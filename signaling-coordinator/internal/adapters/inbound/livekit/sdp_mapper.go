package livekit

import (
	"encoding/json"
	"fmt"
	"sort"
	"strconv"
	"strings"

	"github.com/pion/sdp/v3"

	"ekbatan-room/signaling-coordinator/internal/domain"
)

type ParsedTrack struct {
	Kind              domain.TrackKind
	MID               string
	CID               string
	SSRC              uint32
	RTXSSRC           uint32
	PayloadType       uint32
	RTXPayloadType    uint32
	Codec             string
	ClockRate         uint32
	Channels          uint32
	CName             string
	RtpParametersJSON string
	SimulcastRIDs     []string
}

type ParsedPublisherOffer struct {
	DTLSRole           string // "auto", "client", "server"
	RemoteFingerprints []domain.DtlsFingerprintInfo
	Tracks             []ParsedTrack
}

type ParsedSubscriberAnswer struct {
	DTLSRole           string
	RemoteFingerprints []domain.DtlsFingerprintInfo
}

// supportedRtpHeaderExtensionURIs lists the exact RTP header extension URIs supported
// by Mediasoup's RtpHeaderExtensionUri enum in sfu-media-node (REQ-024). Any extensions
// outside this list (e.g. toffset, video-content-type, video-timing, color-space) will
// trigger deserialization failures in Mediasoup and must be filtered out.
var supportedRtpHeaderExtensionURIs = map[string]bool{
	"urn:ietf:params:rtp-hdrext:sdes:mid":                                                     true,
	"urn:ietf:params:rtp-hdrext:sdes:rtp-stream-id":                                           true,
	"urn:ietf:params:rtp-hdrext:sdes:repaired-rtp-stream-id":                                  true,
	"http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time":                              true,
	"http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01":               true,
	"urn:ietf:params:rtp-hdrext:ssrc-audio-level":                                             true,
	"https://aomediacodec.github.io/av1-rtp-spec/#dependency-descriptor-rtp-header-extension": true,
	"urn:3gpp:video-orientation":                                                              true,
	"http://www.webrtc.org/experiments/rtp-hdrext/abs-capture-time":                           true,
	"http://www.webrtc.org/experiments/rtp-hdrext/playout-delay":                              true,
	"urn:mediasoup:params:rtp-hdrext:packet-id":                                               true,
}

// getOfferDirection extracts the transmission direction from an m-section or the session description.
// Defaults to "sendrecv" per RFC 3264 Section 6.1 if unspecified.
func getOfferDirection(s *sdp.SessionDescription, md *sdp.MediaDescription) string {
	if md.MediaName.Port.Value == 0 {
		return "inactive"
	}
	for _, attr := range md.Attributes {
		k := strings.ToLower(strings.TrimSpace(attr.Key))
		if k == "inactive" || k == "sendonly" || k == "recvonly" || k == "sendrecv" {
			return k
		}
	}
	if s != nil {
		for _, attr := range s.Attributes {
			k := strings.ToLower(strings.TrimSpace(attr.Key))
			if k == "inactive" || k == "sendonly" || k == "recvonly" || k == "sendrecv" {
				return k
			}
		}
	}
	return "sendrecv"
}

// determinePublisherAnswerDirection calculates the conforming SDP answer direction
// for a media section responding to a publisher offer per RFC 3264 Section 6.1.
// Since the SFU acts as an RTP receiver on the publisher peer connection:
// - If the stream is offered as "inactive" (or port is 0), the answer MUST be "inactive".
// - If the stream is offered as "recvonly", the SFU cannot send, so the answer is "inactive".
// - If the stream is offered as "sendonly" or "sendrecv", the SFU responds with "recvonly".
func determinePublisherAnswerDirection(s *sdp.SessionDescription, md *sdp.MediaDescription) string {
	dir := getOfferDirection(s, md)
	switch dir {
	case "inactive", "recvonly":
		return "inactive"
	case "sendonly", "sendrecv":
		return "recvonly"
	default:
		return "recvonly"
	}
}

// ParsePublisherOffer parses the client's publisher SDP offer.
func ParsePublisherOffer(offerSDP string) (*ParsedPublisherOffer, error) {
	var s sdp.SessionDescription
	if err := s.Unmarshal([]byte(offerSDP)); err != nil {
		return nil, fmt.Errorf("failed to unmarshal publisher offer sdp: %w", err)
	}

	fingerprints := make([]domain.DtlsFingerprintInfo, 0)
	dtlsRole := "client" // default client role when client sends actpass

	// 1. Session level fingerprints
	for _, attr := range s.Attributes {
		if attr.Key == "fingerprint" {
			parts := strings.SplitN(attr.Value, " ", 2)
			if len(parts) == 2 {
				fingerprints = append(fingerprints, domain.DtlsFingerprintInfo{
					Algorithm: parts[0],
					Value:     parts[1],
				})
			}
		}
		if strings.EqualFold(attr.Key, "setup") {
			switch strings.TrimSpace(strings.ToLower(attr.Value)) {
			case "actpass", "passive":
				dtlsRole = "client"
			case "active":
				dtlsRole = "server"
			}
		}
	}

	tracks := make([]ParsedTrack, 0)

	// 2. Media descriptions
	for _, md := range s.MediaDescriptions {
		// Media level fingerprints and setup (check for ALL media descriptions, including application)
		for _, attr := range md.Attributes {
			if strings.EqualFold(attr.Key, "fingerprint") {
				parts := strings.SplitN(attr.Value, " ", 2)
				if len(parts) == 2 {
					fingerprints = append(fingerprints, domain.DtlsFingerprintInfo{
						Algorithm: parts[0],
						Value:     parts[1],
					})
				}
			}
			if strings.EqualFold(attr.Key, "setup") {
				switch strings.TrimSpace(strings.ToLower(attr.Value)) {
				case "actpass", "passive":
					dtlsRole = "client"
				case "active":
					dtlsRole = "server"
				}
			}
		}

		kind := domain.TrackKind(md.MediaName.Media)
		if kind != domain.TrackKindAudio && kind != domain.TrackKindVideo {
			continue
		}

		// Inactive or recvonly m-sections do not transmit media from client to server (RFC 3264)
		if getOfferDirection(&s, md) == "inactive" || getOfferDirection(&s, md) == "recvonly" {
			continue
		}

		mid, _ := md.Attribute("mid")

		// Extract SSRC and CName (REQ-021 & REQ-022: order-independent primary media SSRC and RTX SSRC per RFC 5576 / RFC 4588)
		var ssrc uint32
		var rtxSSRC uint32
		cname := "default"

		type fidPair struct {
			media uint32
			rtx   uint32
		}
		var fidPairs []fidPair

		// 1. Scan for a=ssrc-group:FID <mediaSSRC> <rtxSSRC> first (regardless of order in SDP)
		for _, attr := range md.Attributes {
			if strings.EqualFold(attr.Key, "ssrc-group") {
				fields := strings.Fields(attr.Value)
				if len(fields) >= 3 && strings.EqualFold(fields[0], "FID") {
					mediaVal, err1 := strconv.ParseUint(fields[1], 10, 32)
					rtxVal, err2 := strconv.ParseUint(fields[2], 10, 32)
					if err1 == nil && err2 == nil {
						fidPairs = append(fidPairs, fidPair{media: uint32(mediaVal), rtx: uint32(rtxVal)})
						if ssrc == 0 {
							ssrc = uint32(mediaVal)
						}
						if rtxSSRC == 0 {
							rtxSSRC = uint32(rtxVal)
						}
					}
				}
			}
		}

		var cid string
		for _, attr := range md.Attributes {
			if strings.EqualFold(attr.Key, "msid") {
				parts := strings.Fields(attr.Value)
				if len(parts) >= 2 {
					cid = parts[1]
				} else if len(parts) == 1 {
					cid = parts[0]
				}
			}
		}

		// 2. Scan a=ssrc attributes for primary SSRC (if not in FID group), CName, and MSID
		for _, attr := range md.Attributes {
			if strings.EqualFold(attr.Key, "ssrc") {
				parts := strings.Fields(attr.Value)
				if len(parts) >= 1 {
					val, err := strconv.ParseUint(parts[0], 10, 32)
					if err == nil {
						parsedSSRC := uint32(val)
						// If primary ssrc is not yet identified, pick the first SSRC that is not rtxSSRC
						if ssrc == 0 && (rtxSSRC == 0 || parsedSSRC != rtxSSRC) {
							ssrc = parsedSSRC
						}
						// Extract CNAME (preferring CNAME associated with primary media SSRC) and MSID
						if len(parts) >= 2 {
							for _, p := range parts[1:] {
								if strings.HasPrefix(strings.ToLower(p), "cname:") {
									cn := p[6:]
									if ssrc != 0 && parsedSSRC == ssrc {
										cname = cn
									} else if cname == "default" {
										cname = cn
									}
								}
							}
						}
						if strings.Contains(strings.ToLower(attr.Value), "msid:") {
							idx := strings.Index(strings.ToLower(attr.Value), "msid:")
							msidStr := strings.TrimSpace(attr.Value[idx+5:])
							msidFields := strings.Fields(msidStr)
							if len(msidFields) >= 2 {
								if ssrc != 0 && parsedSSRC == ssrc {
									cid = msidFields[1]
								} else if cid == "" {
									cid = msidFields[1]
								}
							} else if len(msidFields) == 1 {
								if ssrc != 0 && parsedSSRC == ssrc {
									cid = msidFields[0]
								} else if cid == "" {
									cid = msidFields[0]
								}
							}
						}
					}
				}
			}
		}

		// Extract primary codec and payload type
		var pt uint32
		var rtxPT uint32
		var codecName string
		var clockRate uint32 = 48000
		var channels uint32
		if kind == domain.TrackKindAudio {
			channels = 2 // default to stereo (2 channels) for WebRTC Opus
		}
		if kind == domain.TrackKindVideo {
			clockRate = 90000
		}

		type parsedCodecInfo struct {
			pt        uint32
			name      string
			clockRate uint32
			channels  uint32
		}
		rtpmapCodecs := make(map[uint32]parsedCodecInfo)
		var rtxCandidates []uint32

		for _, attr := range md.Attributes {
			if strings.EqualFold(attr.Key, "rtpmap") {
				// e.g. "111 opus/48000/2" or "96 VP8/90000"
				parts := strings.SplitN(strings.TrimSpace(attr.Value), " ", 2)
				if len(parts) == 2 {
					payloadNum, err := strconv.ParseUint(parts[0], 10, 32)
					if err == nil {
						subParts := strings.Split(parts[1], "/")
						if len(subParts) >= 2 {
							name := subParts[0]
							rate, _ := strconv.ParseUint(subParts[1], 10, 32)
							var ch uint32
							if len(subParts) >= 3 {
								if parsedChannels, err := strconv.ParseUint(subParts[2], 10, 32); err == nil && parsedChannels > 0 {
									ch = uint32(parsedChannels)
								}
							}
							if strings.EqualFold(name, "rtx") {
								rtxCandidates = append(rtxCandidates, uint32(payloadNum))
							} else if strings.EqualFold(name, "red") || strings.EqualFold(name, "ulpfec") || strings.HasPrefix(strings.ToLower(name), "flexfec") {
								// REQ-025: Skip FEC/redundancy wrapper codecs when determining primary media codec
								continue
							} else {
								rtpmapCodecs[uint32(payloadNum)] = parsedCodecInfo{
									pt:        uint32(payloadNum),
									name:      name,
									clockRate: uint32(rate),
									channels:  ch,
								}
							}
						}
					}
				}
			}
		}

		// Select candidate codecs matching md.MediaName.Formats in declared preference order
		var candidateCodecs []parsedCodecInfo
		for _, f := range md.MediaName.Formats {
			if payloadNum, err := strconv.ParseUint(strings.TrimSpace(f), 10, 32); err == nil {
				if cInfo, ok := rtpmapCodecs[uint32(payloadNum)]; ok {
					candidateCodecs = append(candidateCodecs, cInfo)
				}
			}
		}
		if len(candidateCodecs) == 0 {
			for _, cInfo := range rtpmapCodecs {
				candidateCodecs = append(candidateCodecs, cInfo)
			}
		}

		// Select the codec with highest router preference (VP8 > H264 for video to avoid "Unsupported codec" per REQ-003)
		if len(candidateCodecs) > 0 {
			bestIdx := 0
			bestScore := -1
			for i, c := range candidateCodecs {
				score := 10
				if kind == domain.TrackKindVideo {
					score = scoreVideoCodec(c.name)
				} else if kind == domain.TrackKindAudio {
					score = scoreAudioCodec(c.name)
				}
				if score > bestScore {
					bestScore = score
					bestIdx = i
				}
			}
			chosen := candidateCodecs[bestIdx]
			pt = chosen.pt
			codecName = chosen.name
			if chosen.clockRate > 0 {
				clockRate = chosen.clockRate
			}
			if chosen.channels > 0 {
				channels = chosen.channels
			}
		}

		if pt == 0 && len(md.MediaName.Formats) > 0 {
			if parsedPT, err := strconv.ParseUint(md.MediaName.Formats[0], 10, 32); err == nil {
				pt = uint32(parsedPT)
			}
		}

		if codecName == "" {
			if kind == domain.TrackKindAudio {
				codecName = "opus"
				pt = 111
				channels = 2
			} else {
				codecName = "VP8"
				pt = 96
			}
		}

		// Parse fmtp parameters for primary payload type and map RTX apt associations (RFC 4588)
		var codecParams map[string]interface{}
		fmtpAptMap := make(map[uint32]uint32)
		for _, attr := range md.Attributes {
			if strings.EqualFold(attr.Key, "fmtp") {
				parts := strings.SplitN(strings.TrimSpace(attr.Value), " ", 2)
				if len(parts) == 2 {
					fmtpPT, err := strconv.ParseUint(parts[0], 10, 32)
					if err == nil {
						paramPairs := strings.Split(parts[1], ";")
						isPrimaryCodec := (uint32(fmtpPT) == pt)
						if isPrimaryCodec {
							codecParams = make(map[string]interface{})
						}
						for _, pair := range paramPairs {
							kv := strings.SplitN(strings.TrimSpace(pair), "=", 2)
							if len(kv) == 2 {
								k := strings.TrimSpace(kv[0])
								v := strings.TrimSpace(kv[1])
								if strings.EqualFold(k, "apt") {
									if aptVal, err := strconv.ParseUint(v, 10, 32); err == nil {
										fmtpAptMap[uint32(fmtpPT)] = uint32(aptVal)
									}
								}
								if isPrimaryCodec {
									if intVal, err := strconv.Atoi(v); err == nil {
										codecParams[k] = intVal
									} else {
										codecParams[k] = v
									}
								}
							} else if len(kv) == 1 && kv[0] != "" && isPrimaryCodec {
								codecParams[strings.TrimSpace(kv[0])] = 1
							}
						}
					}
				}
			}
		}

		// Match RTX payload type for the primary codec via fmtp apt (RFC 4588)
		for _, cand := range rtxCandidates {
			if aptVal, exists := fmtpAptMap[cand]; exists && aptVal == pt {
				rtxPT = cand
				break
			}
		}
		// Fallback: If no apt was matched, select an RTX candidate only if it does not belong to another codec
		if rtxPT == 0 && len(rtxCandidates) > 0 {
			for _, cand := range rtxCandidates {
				if aptVal, exists := fmtpAptMap[cand]; !exists || aptVal == pt {
					rtxPT = cand
					break
				}
			}
		}

		// Build RTP parameters JSON for SFU node
		codecEntry := map[string]interface{}{
			"mimeType":    fmt.Sprintf("%s/%s", kind, codecName),
			"payloadType": pt,
			"clockRate":   clockRate,
		}
		if kind == domain.TrackKindAudio {
			if channels == 0 {
				channels = 2
			}
			codecEntry["channels"] = channels
		}
		if len(codecParams) > 0 {
			codecEntry["parameters"] = codecParams
		}

		// REQ-020: Parse RTCP feedback attributes or provide standard video feedback
		var rtcpFeedback []map[string]interface{}
		hasPLI := false
		hasFIR := false
		for _, attr := range md.Attributes {
			if attr.Key == "rtcp-fb" {
				parts := strings.Fields(attr.Value)
				if len(parts) >= 2 {
					fbPT := parts[0]
					if fbPT == strconv.FormatUint(uint64(pt), 10) || fbPT == "*" {
						fbType := parts[1]
						entry := map[string]interface{}{
							"type": fbType,
						}
						if len(parts) >= 3 {
							entry["parameter"] = parts[2]
							if fbType == "nack" && parts[2] == "pli" {
								hasPLI = true
							}
							if fbType == "ccm" && parts[2] == "fir" {
								hasFIR = true
							}
						}
						rtcpFeedback = append(rtcpFeedback, entry)
					}
				}
			}
		}

		if kind == domain.TrackKindVideo {
			if len(rtcpFeedback) == 0 {
				rtcpFeedback = []map[string]interface{}{
					{"type": "nack"},
					{"type": "nack", "parameter": "pli"},
					{"type": "ccm", "parameter": "fir"},
					{"type": "goog-remb"},
					{"type": "transport-cc"},
				}
			} else {
				if !hasPLI {
					rtcpFeedback = append(rtcpFeedback, map[string]interface{}{"type": "nack", "parameter": "pli"})
				}
				if !hasFIR {
					rtcpFeedback = append(rtcpFeedback, map[string]interface{}{"type": "ccm", "parameter": "fir"})
				}
			}
		}

		if len(rtcpFeedback) > 0 {
			codecEntry["rtcpFeedback"] = rtcpFeedback
		}

		rtcpParams := map[string]interface{}{
			"reducedSize": true,
		}
		if cname != "" {
			rtcpParams["cname"] = cname
		}

		codecs := []map[string]interface{}{
			codecEntry,
		}
		if rtxPT > 0 {
			codecs = append(codecs, map[string]interface{}{
				"mimeType":    fmt.Sprintf("%s/rtx", kind),
				"payloadType": rtxPT,
				"clockRate":   clockRate,
				"parameters": map[string]interface{}{
					"apt": pt,
				},
			})
		}

		// Parse header extensions (a=extmap) from media and session levels (media overrides session per RFC 8866)
		extMap := make(map[int]string)
		for _, attr := range s.Attributes {
			if strings.EqualFold(attr.Key, "extmap") {
				fields := strings.Fields(attr.Value)
				if len(fields) >= 2 {
					idStr := strings.Split(fields[0], "/")[0]
					uri := fields[1]
					if id, err := strconv.Atoi(idStr); err == nil && supportedRtpHeaderExtensionURIs[uri] {
						extMap[id] = uri
					}
				}
			}
		}
		for _, attr := range md.Attributes {
			if strings.EqualFold(attr.Key, "extmap") {
				fields := strings.Fields(attr.Value)
				if len(fields) >= 2 {
					idParts := strings.Split(fields[0], "/")
					idStr := idParts[0]
					if id, err := strconv.Atoi(idStr); err == nil {
						if len(idParts) > 1 && strings.EqualFold(idParts[1], "inactive") {
							delete(extMap, id)
							continue
						}
						uri := fields[1]
						if supportedRtpHeaderExtensionURIs[uri] {
							extMap[id] = uri
						} else {
							delete(extMap, id)
						}
					}
				}
			}
		}

		extIDs := make([]int, 0, len(extMap))
		for id := range extMap {
			extIDs = append(extIDs, id)
		}
		sort.Ints(extIDs)

		headerExtensions := make([]map[string]interface{}, 0, len(extIDs))
		for _, id := range extIDs {
			headerExtensions = append(headerExtensions, map[string]interface{}{
				"id":      id,
				"uri":     extMap[id],
				"encrypt": false,
			})
		}

		// Parse Simulcast RIDs (a=rid)
		var simulcastRIDs []string
		for _, attr := range md.Attributes {
			if strings.EqualFold(attr.Key, "rid") {
				fields := strings.Fields(attr.Value)
				if len(fields) >= 1 {
					simulcastRIDs = append(simulcastRIDs, fields[0])
				}
			}
		}

		encodings := make([]map[string]interface{}, 0)
		if len(simulcastRIDs) > 0 {
			for idx, rid := range simulcastRIDs {
				enc := map[string]interface{}{
					"rid": rid,
				}
				if idx < len(fidPairs) {
					enc["ssrc"] = fidPairs[idx].media
					if fidPairs[idx].rtx > 0 {
						enc["rtx"] = map[string]interface{}{
							"ssrc": fidPairs[idx].rtx,
						}
					}
				} else if idx == 0 && ssrc > 0 {
					enc["ssrc"] = ssrc
					if rtxSSRC > 0 {
						enc["rtx"] = map[string]interface{}{
							"ssrc": rtxSSRC,
						}
					}
				}
				encodings = append(encodings, enc)
			}
		} else {
			enc := map[string]interface{}{}
			if ssrc > 0 {
				enc["ssrc"] = ssrc
			}
			if rtxSSRC > 0 {
				enc["rtx"] = map[string]interface{}{
					"ssrc": rtxSSRC,
				}
			}
			encodings = append(encodings, enc)
		}

		rtpParams := map[string]interface{}{
			"codecs":            codecs,
			"headerExtensions":  headerExtensions,
			"header_extensions": headerExtensions,
			"encodings":         encodings,
			"rtcp":              rtcpParams,
		}
		if mid != "" {
			rtpParams["mid"] = mid
		}
		rtpJSON, _ := json.Marshal(rtpParams)

		tracks = append(tracks, ParsedTrack{
			Kind:              kind,
			MID:               mid,
			CID:               cid,
			SSRC:              ssrc,
			RTXSSRC:           rtxSSRC,
			PayloadType:       pt,
			RTXPayloadType:    rtxPT,
			Codec:             codecName,
			ClockRate:         clockRate,
			Channels:          channels,
			CName:             cname,
			RtpParametersJSON: string(rtpJSON),
			SimulcastRIDs:     simulcastRIDs,
		})
	}

	return &ParsedPublisherOffer{
		DTLSRole:           dtlsRole,
		RemoteFingerprints: fingerprints,
		Tracks:             tracks,
	}, nil
}

// scoreVideoCodec assigns a preference score to video codecs.
// VP8 is assigned highest priority to ensure compatibility with SFU router capabilities,
// prioritizing it over H264 to prevent "Unsupported codec" errors during single-layer / load tests (REQ-003).
func scoreVideoCodec(name string) int {
	switch strings.ToUpper(strings.TrimSpace(name)) {
	case "VP8":
		return 100
	case "VP9":
		return 90
	case "AV1":
		return 80
	case "H264":
		return 50
	default:
		return 10
	}
}

// scoreAudioCodec assigns a preference score to audio codecs.
func scoreAudioCodec(name string) int {
	switch strings.ToLower(strings.TrimSpace(name)) {
	case "opus":
		return 100
	default:
		return 10
	}
}

// selectPrimaryFingerprint selects a single DTLS fingerprint from the available list per RFC 8122,
// prioritizing sha-256 (standard WebRTC default), then sha-512, sha-384, or the first available.
func selectPrimaryFingerprint(fps []domain.DtlsFingerprintInfo) *domain.DtlsFingerprintInfo {
	if len(fps) == 0 {
		return nil
	}
	for i := range fps {
		if strings.EqualFold(fps[i].Algorithm, "sha-256") {
			return &fps[i]
		}
	}
	for i := range fps {
		if strings.EqualFold(fps[i].Algorithm, "sha-512") || strings.EqualFold(fps[i].Algorithm, "sha-384") {
			return &fps[i]
		}
	}
	return &fps[0]
}

// findExcludedVideoPayloadTypes identifies video payload types that must be excluded from SDP answers,
// specifically RFC 2198 (RED) and RFC 5109 (ULPFEC) codecs and their associated RTX retransmission pairs (RFC 4588).
// When negotiated in an SDP answer, Chromium encapsulates video frames in RED (PT 102), which causes
// Mediasoup's SFU producer to drop packets due to payload type mismatch with the primary codec (e.g. VP8 PT 96).
func findExcludedVideoPayloadTypes(md *sdp.MediaDescription) map[string]bool {
	ptToCodec := make(map[string]string)
	rtxToApt := make(map[string]string)

	for _, attr := range md.Attributes {
		if strings.EqualFold(attr.Key, "rtpmap") {
			parts := strings.SplitN(strings.TrimSpace(attr.Value), " ", 2)
			if len(parts) == 2 {
				pt := parts[0]
				subParts := strings.Split(parts[1], "/")
				if len(subParts) >= 1 {
					ptToCodec[pt] = strings.ToLower(strings.TrimSpace(subParts[0]))
				}
			}
		} else if strings.EqualFold(attr.Key, "fmtp") {
			parts := strings.SplitN(strings.TrimSpace(attr.Value), " ", 2)
			if len(parts) == 2 {
				fmtpPT := parts[0]
				paramPairs := strings.Split(parts[1], ";")
				for _, pair := range paramPairs {
					kv := strings.SplitN(strings.TrimSpace(pair), "=", 2)
					if len(kv) == 2 && strings.EqualFold(strings.TrimSpace(kv[0]), "apt") {
						rtxToApt[fmtpPT] = strings.TrimSpace(kv[1])
					}
				}
			}
		}
	}

	excluded := make(map[string]bool)
	for pt, codec := range ptToCodec {
		if codec == "red" || codec == "ulpfec" || strings.HasPrefix(codec, "flexfec") {
			excluded[pt] = true
		}
	}

	for rtxPT, aptPT := range rtxToApt {
		if excluded[aptPT] {
			excluded[rtxPT] = true
		}
	}

	return excluded
}

// GeneratePublisherAnswer generates an SDP answer responding to the client's publisher offer.
// Crucial DTLS requirement: always responds with a=setup:passive.
func GeneratePublisherAnswer(offerSDP string, transport *domain.TransportInfo) (string, error) {
	var s sdp.SessionDescription
	if err := s.Unmarshal([]byte(offerSDP)); err != nil {
		return "", fmt.Errorf("failed to unmarshal offer for answer generation: %w", err)
	}

	ans := sdp.SessionDescription{
		Version: 0,
		Origin: sdp.Origin{
			Username:       "-",
			SessionID:      s.Origin.SessionID,
			SessionVersion: s.Origin.SessionVersion + 1,
			NetworkType:    "IN",
			AddressType:    "IP4",
			UnicastAddress: "127.0.0.1",
		},
		SessionName: sdp.SessionName("-"),
		ConnectionInformation: &sdp.ConnectionInformation{
			NetworkType: "IN",
			AddressType: "IP4",
			Address:     &sdp.Address{Address: "127.0.0.1"},
		},
		TimeDescriptions: []sdp.TimeDescription{
			{Timing: sdp.Timing{StartTime: 0, StopTime: 0}},
		},
	}

	// Group BUNDLE attribute (excluding rejected port 0 m-sections per RFC 8843 Section 7.3)
	mids := make([]string, 0, len(s.MediaDescriptions))
	for _, md := range s.MediaDescriptions {
		if md.MediaName.Port.Value == 0 {
			continue
		}
		if mid, ok := md.Attribute("mid"); ok {
			mids = append(mids, mid)
		}
	}
	if len(mids) > 0 {
		ans.Attributes = append(ans.Attributes, sdp.Attribute{
			Key:   "group",
			Value: "BUNDLE " + strings.Join(mids, " "),
		})
	}

	// Session level ICE, DTLS and fingerprints (single fingerprint per RFC 8122)
	ans.Attributes = append(ans.Attributes,
		sdp.Attribute{Key: "ice-ufrag", Value: transport.IceUfrag},
		sdp.Attribute{Key: "ice-pwd", Value: transport.IcePwd},
		sdp.Attribute{Key: "ice-options", Value: "trickle"},
		sdp.Attribute{Key: "ice-lite"},
		sdp.Attribute{Key: "setup", Value: "active"}, // REQ-005: active DTLS initiator role
	)

	if fp := selectPrimaryFingerprint(transport.DtlsFingerprints); fp != nil {
		ans.Attributes = append(ans.Attributes, sdp.Attribute{
			Key:   "fingerprint",
			Value: fmt.Sprintf("%s %s", fp.Algorithm, fp.Value),
		})
	}

	// Build media descriptions answering each offer m-line
	for _, md := range s.MediaDescriptions {
		mid, _ := md.Attribute("mid")
		isApp := strings.EqualFold(md.MediaName.Media, "application")
		isVideo := strings.EqualFold(md.MediaName.Media, "video")

		var excludedPTs map[string]bool
		if isVideo {
			excludedPTs = findExcludedVideoPayloadTypes(md)
		}

		protos := []string{"UDP/TLS/RTP/SAVPF"}
		if isApp && len(md.MediaName.Protos) > 0 {
			protos = md.MediaName.Protos
		}

		formats := md.MediaName.Formats
		if isVideo && len(excludedPTs) > 0 {
			filteredFormats := make([]string, 0, len(md.MediaName.Formats))
			for _, f := range md.MediaName.Formats {
				if !excludedPTs[f] {
					filteredFormats = append(filteredFormats, f)
				}
			}
			formats = filteredFormats
		}

		port := 9
		if md.MediaName.Port.Value == 0 {
			port = 0
		}

		ansMd := &sdp.MediaDescription{
			MediaName: sdp.MediaName{
				Media:   md.MediaName.Media,
				Port:    sdp.RangedPort{Value: port},
				Protos:  protos,
				Formats: formats,
			},
			ConnectionInformation: &sdp.ConnectionInformation{
				NetworkType: "IN",
				AddressType: "IP4",
				Address:     &sdp.Address{Address: "127.0.0.1"},
			},
		}

		if mid != "" {
			ansMd.Attributes = append(ansMd.Attributes, sdp.Attribute{Key: "mid", Value: mid})
		}

		// REQ-005: Enforce media-level a=setup:active in every media section
		// (m=application, m=video, m=audio) to prevent DTLS role deadlock with Pion WebRTC clients.
		ansMd.Attributes = append(ansMd.Attributes, sdp.Attribute{Key: "setup", Value: "active"})

		if !isApp {
			// RFC 3264 Section 6.1: Conform direction (inactive if offer is inactive/recvonly/port 0, otherwise recvonly)
			ansDir := determinePublisherAnswerDirection(&s, md)
			ansMd.Attributes = append(ansMd.Attributes, sdp.Attribute{Key: ansDir})
			ansMd.Attributes = append(ansMd.Attributes, sdp.Attribute{Key: "rtcp-mux"})

			// Retain rtpmap, rtcp-fb, fmtp, and extmap from offer per RFC 8285 / RFC 8866
			// REQ-025: Filter out red, ulpfec, and their RTX pairs from video SDP answers
			for _, attr := range md.Attributes {
				k := strings.ToLower(attr.Key)
				if k == "rtpmap" || k == "fmtp" || k == "rtcp-fb" {
					if isVideo && len(excludedPTs) > 0 {
						fields := strings.Fields(attr.Value)
						if len(fields) > 0 && fields[0] != "*" && excludedPTs[fields[0]] {
							continue
						}
					}
					ansMd.Attributes = append(ansMd.Attributes, attr)
				} else if k == "extmap" {
					ansMd.Attributes = append(ansMd.Attributes, attr)
				} else if k == "rid" {
					// REQ-002: Negotiate simulcast RID: offer has "rid:<id> send ...", answer must have "rid:<id> recv ..."
					fields := strings.Fields(attr.Value)
					val := attr.Value
					if len(fields) >= 2 && strings.EqualFold(fields[1], "send") {
						fields[1] = "recv"
						val = strings.Join(fields, " ")
					} else if len(fields) == 1 {
						val = fields[0] + " recv"
					}
					ansMd.Attributes = append(ansMd.Attributes, sdp.Attribute{
						Key:   "rid",
						Value: val,
					})
				} else if k == "simulcast" {
					// REQ-002: RFC 8853 Section 5.2: If offer has "simulcast:send ...", answer must have "simulcast:recv ..."
					trimmed := strings.TrimSpace(attr.Value)
					var ansVal string
					parts := strings.Fields(trimmed)
					for i, p := range parts {
						if strings.EqualFold(p, "send") && i+1 < len(parts) {
							ansVal = "recv " + parts[i+1]
							break
						}
					}
					if ansVal == "" {
						if strings.HasPrefix(strings.ToLower(trimmed), "send ") {
							ansVal = "recv " + strings.TrimSpace(trimmed[5:])
						} else if strings.EqualFold(trimmed, "send") {
							ansVal = "recv"
						}
					}
					if ansVal != "" {
						ansMd.Attributes = append(ansMd.Attributes, sdp.Attribute{
							Key:   "simulcast",
							Value: ansVal,
						})
					}
				}
			}
		} else {
			// Retain sctp-port and max-message-size from offer
			for _, attr := range md.Attributes {
				if attr.Key == "sctp-port" || attr.Key == "max-message-size" {
					ansMd.Attributes = append(ansMd.Attributes, attr)
				}
			}
		}

		// Add server ICE candidates
		for _, cand := range transport.IceCandidates {
			if cand.SDPCandidateLine != "" {
				ansMd.Attributes = append(ansMd.Attributes, sdp.Attribute{
					Key:   "candidate",
					Value: strings.TrimPrefix(cand.SDPCandidateLine, "candidate:"),
				})
			}
		}

		ans.MediaDescriptions = append(ans.MediaDescriptions, ansMd)
	}

	raw, err := ans.Marshal()
	if err != nil {
		return "", fmt.Errorf("failed to marshal publisher answer: %w", err)
	}
	return string(raw), nil
}

// GenerateSubscriberOffer synthesizes an SDP offer for the subscriber peer connection.
// Unified Plan ordering is strictly maintained using SDPSlots. Inactive slots use port 0 and a=inactive.
func GenerateSubscriberOffer(transport *domain.TransportInfo, slots []*domain.SDPSlot, participantSID string, sessionVersion ...uint64) (string, error) {
	var sVer uint64 = 1
	if len(sessionVersion) > 0 && sessionVersion[0] > 0 {
		sVer = sessionVersion[0]
	}

	offer := sdp.SessionDescription{
		Version: 0,
		Origin: sdp.Origin{
			Username:       "-",
			SessionID:      1000000000,
			SessionVersion: sVer,
			NetworkType:    "IN",
			AddressType:    "IP4",
			UnicastAddress: "127.0.0.1",
		},
		SessionName: sdp.SessionName("-"),
		ConnectionInformation: &sdp.ConnectionInformation{
			NetworkType: "IN",
			AddressType: "IP4",
			Address:     &sdp.Address{Address: "127.0.0.1"},
		},
		TimeDescriptions: []sdp.TimeDescription{
			{Timing: sdp.Timing{StartTime: 0, StopTime: 0}},
		},
	}

	activeMids := make([]string, 0, len(slots))
	for _, slot := range slots {
		if slot.Active {
			activeMids = append(activeMids, slot.MID)
		}
	}
	if len(activeMids) > 0 {
		offer.Attributes = append(offer.Attributes, sdp.Attribute{
			Key:   "group",
			Value: "BUNDLE " + strings.Join(activeMids, " "),
		})
	}

	offer.Attributes = append(offer.Attributes,
		sdp.Attribute{Key: "msid-semantic", Value: "WMS"},
		sdp.Attribute{Key: "ice-ufrag", Value: transport.IceUfrag},
		sdp.Attribute{Key: "ice-pwd", Value: transport.IcePwd},
		sdp.Attribute{Key: "ice-options", Value: "trickle"},
		sdp.Attribute{Key: "ice-lite"},
		sdp.Attribute{Key: "setup", Value: "actpass"},
		sdp.Attribute{Key: "extmap", Value: "1 urn:ietf:params:rtp-hdrext:sdes:mid"},
		sdp.Attribute{Key: "extmap", Value: "2 http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01"},
	)

	if fp := selectPrimaryFingerprint(transport.DtlsFingerprints); fp != nil {
		offer.Attributes = append(offer.Attributes, sdp.Attribute{
			Key:   "fingerprint",
			Value: fmt.Sprintf("%s %s", fp.Algorithm, fp.Value),
		})
	}

	for _, slot := range slots {
		md := &sdp.MediaDescription{
			MediaName: sdp.MediaName{
				Media:  string(slot.Kind),
				Protos: []string{"UDP/TLS/RTP/SAVPF"},
			},
			ConnectionInformation: &sdp.ConnectionInformation{
				NetworkType: "IN",
				AddressType: "IP4",
				Address:     &sdp.Address{Address: "127.0.0.1"},
			},
		}

		if !slot.Active {
			// Inactive slot (recycled or closed track): port 0, a=inactive
			md.MediaName.Port = sdp.RangedPort{Value: 0}
			md.MediaName.Formats = []string{fmt.Sprintf("%d", slot.PayloadType)}
			md.Attributes = append(md.Attributes,
				sdp.Attribute{Key: "mid", Value: slot.MID},
				sdp.Attribute{Key: "inactive"},
			)
			offer.MediaDescriptions = append(offer.MediaDescriptions, md)
			continue
		}

		// Active slot: port 9, a=sendonly
		md.MediaName.Port = sdp.RangedPort{Value: 9}
		md.MediaName.Formats = []string{fmt.Sprintf("%d", slot.PayloadType)}
		if slot.RTXPayloadType > 0 {
			md.MediaName.Formats = append(md.MediaName.Formats, fmt.Sprintf("%d", slot.RTXPayloadType))
		}

		codecName := slot.MimeType
		if idx := strings.Index(codecName, "/"); idx != -1 {
			codecName = codecName[idx+1:]
		}
		rtpmapVal := fmt.Sprintf("%d %s/%d", slot.PayloadType, codecName, slot.ClockRate)
		if slot.Kind == domain.TrackKindAudio && strings.EqualFold(codecName, "opus") {
			rtpmapVal = fmt.Sprintf("%d %s/%d/2", slot.PayloadType, codecName, slot.ClockRate)
		}

		md.Attributes = append(md.Attributes,
			sdp.Attribute{Key: "mid", Value: slot.MID},
			sdp.Attribute{Key: "sendonly"},
			sdp.Attribute{Key: "rtcp-mux"},
			sdp.Attribute{
				Key:   "rtpmap",
				Value: rtpmapVal,
			},
		)
		if strings.EqualFold(codecName, "opus") {
			md.Attributes = append(md.Attributes, sdp.Attribute{
				Key:   "fmtp",
				Value: fmt.Sprintf("%d minptime=10;useinbandfec=1", slot.PayloadType),
			})
		}
		if slot.Kind == domain.TrackKindVideo {
			md.Attributes = append(md.Attributes,
				sdp.Attribute{Key: "rtcp-fb", Value: fmt.Sprintf("%d goog-remb", slot.PayloadType)},
				sdp.Attribute{Key: "rtcp-fb", Value: fmt.Sprintf("%d transport-cc", slot.PayloadType)},
				sdp.Attribute{Key: "rtcp-fb", Value: fmt.Sprintf("%d ccm fir", slot.PayloadType)},
				sdp.Attribute{Key: "rtcp-fb", Value: fmt.Sprintf("%d nack", slot.PayloadType)},
				sdp.Attribute{Key: "rtcp-fb", Value: fmt.Sprintf("%d nack pli", slot.PayloadType)},
			)
		}

		if slot.RTXPayloadType > 0 {
			md.Attributes = append(md.Attributes,
				sdp.Attribute{
					Key:   "rtpmap",
					Value: fmt.Sprintf("%d rtx/%d", slot.RTXPayloadType, slot.ClockRate),
				},
				sdp.Attribute{
					Key:   "fmtp",
					Value: fmt.Sprintf("%d apt=%d", slot.RTXPayloadType, slot.PayloadType),
				},
			)
		}

		pubSID := slot.PublisherSID
		if pubSID == "" {
			pubSID = slot.CName
		}
		if pubSID == "" {
			pubSID = participantSID
		}

		// Modern WebRTC Unified Plan (RFC 8829 / LiveKit protocol) uses BUNDLE and a=mid
		// for track demultiplexing. Explicit a=ssrc and a=ssrc-group lines MUST NOT be
		// emitted in subscriber offers (REQ-001) as they cause Pion WebRTC (livekit-cli)
		// to reject incoming RTP packets with mismatched SSRCs and drop tracks.
		md.Attributes = append(md.Attributes,
			sdp.Attribute{
				Key:   "msid",
				Value: fmt.Sprintf("%s %s", pubSID, slot.TrackSID),
			},
		)

		// Add candidates
		for _, cand := range transport.IceCandidates {
			if cand.SDPCandidateLine != "" {
				md.Attributes = append(md.Attributes, sdp.Attribute{
					Key:   "candidate",
					Value: strings.TrimPrefix(cand.SDPCandidateLine, "candidate:"),
				})
			}
		}

		offer.MediaDescriptions = append(offer.MediaDescriptions, md)
	}

	raw, err := offer.Marshal()
	if err != nil {
		return "", fmt.Errorf("failed to marshal subscriber offer: %w", err)
	}
	return string(raw), nil
}

// ParseSubscriberAnswer extracts DTLS role and remote fingerprints from client's subscriber answer.
func ParseSubscriberAnswer(answerSDP string) (*ParsedSubscriberAnswer, error) {
	var s sdp.SessionDescription
	if err := s.Unmarshal([]byte(answerSDP)); err != nil {
		return nil, fmt.Errorf("failed to unmarshal subscriber answer: %w", err)
	}

	fingerprints := make([]domain.DtlsFingerprintInfo, 0)
	dtlsRole := "client"

	for _, attr := range s.Attributes {
		if attr.Key == "fingerprint" {
			parts := strings.SplitN(attr.Value, " ", 2)
			if len(parts) == 2 {
				fingerprints = append(fingerprints, domain.DtlsFingerprintInfo{
					Algorithm: parts[0],
					Value:     parts[1],
				})
			}
		}
		if attr.Key == "setup" {
			switch strings.TrimSpace(attr.Value) {
			case "active":
				dtlsRole = "client"
			case "passive":
				dtlsRole = "server"
			}
		}
	}

	for _, md := range s.MediaDescriptions {
		for _, attr := range md.Attributes {
			if attr.Key == "fingerprint" {
				parts := strings.SplitN(attr.Value, " ", 2)
				if len(parts) == 2 {
					fingerprints = append(fingerprints, domain.DtlsFingerprintInfo{
						Algorithm: parts[0],
						Value:     parts[1],
					})
				}
			}
			if attr.Key == "setup" {
				switch strings.TrimSpace(attr.Value) {
				case "active":
					dtlsRole = "client"
				case "passive":
					dtlsRole = "server"
				}
			}
		}
	}

	return &ParsedSubscriberAnswer{
		DTLSRole:           dtlsRole,
		RemoteFingerprints: fingerprints,
	}, nil
}
