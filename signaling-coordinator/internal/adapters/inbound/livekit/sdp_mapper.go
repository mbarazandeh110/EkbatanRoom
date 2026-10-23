package livekit

import (
	"encoding/json"
	"fmt"
	"strconv"
	"strings"

	"github.com/pion/sdp/v3"

	"ekbatan-room/signaling-coordinator/internal/domain"
)

type ParsedTrack struct {
	Kind              domain.TrackKind
	MID               string
	SSRC              uint32
	RTXSSRC           uint32
	PayloadType       uint32
	RTXPayloadType    uint32
	Codec             string
	ClockRate         uint32
	Channels          uint32
	CName             string
	RtpParametersJSON string
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
		if attr.Key == "setup" {
			switch strings.TrimSpace(attr.Value) {
			case "actpass", "active":
				dtlsRole = "client"
			case "passive":
				dtlsRole = "server"
			}
		}
	}

	tracks := make([]ParsedTrack, 0)

	// 2. Media descriptions
	for _, md := range s.MediaDescriptions {
		// Media level fingerprints and setup (check for ALL media descriptions, including application)
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
				case "actpass", "active":
					dtlsRole = "client"
				case "passive":
					dtlsRole = "server"
				}
			}
		}

		kind := domain.TrackKind(md.MediaName.Media)
		if kind != domain.TrackKindAudio && kind != domain.TrackKindVideo {
			continue
		}

		mid, _ := md.Attribute("mid")

		// Extract SSRC and CName
		var ssrc uint32
		var rtxSSRC uint32
		cname := "default"

		for _, attr := range md.Attributes {
			if attr.Key == "ssrc" {
				parts := strings.Split(attr.Value, " ")
				if len(parts) >= 1 {
					val, err := strconv.ParseUint(parts[0], 10, 32)
					if err == nil && ssrc == 0 {
						ssrc = uint32(val)
					}
				}
				if len(parts) >= 2 && strings.HasPrefix(parts[1], "cname:") {
					cname = strings.TrimPrefix(parts[1], "cname:")
				}
			}
			if attr.Key == "ssrc-group" && strings.HasPrefix(attr.Value, "FID ") {
				parts := strings.Split(attr.Value, " ")
				if len(parts) >= 3 {
					if rtxVal, err := strconv.ParseUint(parts[2], 10, 32); err == nil {
						rtxSSRC = uint32(rtxVal)
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

		for _, attr := range md.Attributes {
			if attr.Key == "rtpmap" {
				// e.g. "111 opus/48000/2" or "96 VP8/90000"
				parts := strings.SplitN(attr.Value, " ", 2)
				if len(parts) == 2 {
					payloadNum, err := strconv.ParseUint(parts[0], 10, 32)
					if err == nil {
						subParts := strings.Split(parts[1], "/")
						if len(subParts) >= 2 {
							name := subParts[0]
							rate, _ := strconv.ParseUint(subParts[1], 10, 32)
							if strings.EqualFold(name, "rtx") {
								rtxPT = uint32(payloadNum)
							} else if pt == 0 {
								pt = uint32(payloadNum)
								codecName = name
								if rate > 0 {
									clockRate = uint32(rate)
								}
								if len(subParts) >= 3 {
									if parsedChannels, err := strconv.ParseUint(subParts[2], 10, 32); err == nil && parsedChannels > 0 {
										channels = uint32(parsedChannels)
									}
								}
							}
						}
					}
				}
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

		rtcpParams := map[string]interface{}{
			"reducedSize": true,
		}
		if cname != "" {
			rtcpParams["cname"] = cname
		}

		rtpParams := map[string]interface{}{
			"codecs": []map[string]interface{}{
				codecEntry,
			},
			"headerExtensions": []map[string]interface{}{},
			"encodings": []map[string]interface{}{
				{
					"ssrc": ssrc,
				},
			},
			"rtcp": rtcpParams,
		}
		if mid != "" {
			rtpParams["mid"] = mid
		}
		if rtxSSRC > 0 {
			rtpParams["encodings"].([]map[string]interface{})[0]["rtx"] = map[string]interface{}{
				"ssrc": rtxSSRC,
			}
		}
		rtpJSON, _ := json.Marshal(rtpParams)

		tracks = append(tracks, ParsedTrack{
			Kind:              kind,
			MID:               mid,
			SSRC:              ssrc,
			RTXSSRC:           rtxSSRC,
			PayloadType:       pt,
			RTXPayloadType:    rtxPT,
			Codec:             codecName,
			ClockRate:         clockRate,
			Channels:          channels,
			CName:             cname,
			RtpParametersJSON: string(rtpJSON),
		})
	}

	return &ParsedPublisherOffer{
		DTLSRole:           dtlsRole,
		RemoteFingerprints: fingerprints,
		Tracks:             tracks,
	}, nil
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

	// Group BUNDLE attribute
	mids := make([]string, 0, len(s.MediaDescriptions))
	for _, md := range s.MediaDescriptions {
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
		sdp.Attribute{Key: "setup", Value: "passive"},
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

		protos := []string{"UDP/TLS/RTP/SAVPF"}
		if isApp && len(md.MediaName.Protos) > 0 {
			protos = md.MediaName.Protos
		}

		ansMd := &sdp.MediaDescription{
			MediaName: sdp.MediaName{
				Media:   md.MediaName.Media,
				Port:    sdp.RangedPort{Value: 9},
				Protos:  protos,
				Formats: md.MediaName.Formats,
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

		if !isApp {
			ansMd.Attributes = append(ansMd.Attributes, sdp.Attribute{Key: "recvonly"})
			ansMd.Attributes = append(ansMd.Attributes, sdp.Attribute{Key: "rtcp-mux"})

			// Retain rtpmap and rtcp-fb from offer
			for _, attr := range md.Attributes {
				if attr.Key == "rtpmap" || attr.Key == "rtcp-fb" || attr.Key == "fmtp" {
					ansMd.Attributes = append(ansMd.Attributes, attr)
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
func GenerateSubscriberOffer(transport *domain.TransportInfo, slots []*domain.SDPSlot, participantSID string) (string, error) {
	offer := sdp.SessionDescription{
		Version: 0,
		Origin: sdp.Origin{
			Username:       "-",
			SessionID:      1000000000,
			SessionVersion: 1,
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

	mids := make([]string, 0, len(slots))
	for _, slot := range slots {
		mids = append(mids, slot.MID)
	}
	if len(mids) > 0 {
		offer.Attributes = append(offer.Attributes, sdp.Attribute{
			Key:   "group",
			Value: "BUNDLE " + strings.Join(mids, " "),
		})
	}

	offer.Attributes = append(offer.Attributes,
		sdp.Attribute{Key: "msid-semantic", Value: "WMS"},
		sdp.Attribute{Key: "ice-ufrag", Value: transport.IceUfrag},
		sdp.Attribute{Key: "ice-pwd", Value: transport.IcePwd},
		sdp.Attribute{Key: "ice-options", Value: "trickle"},
		sdp.Attribute{Key: "setup", Value: "actpass"},
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

		cname := slot.CName
		if cname == "" {
			cname = participantSID
		}

		md.Attributes = append(md.Attributes,
			sdp.Attribute{
				Key:   "msid",
				Value: fmt.Sprintf("%s %s", participantSID, slot.TrackSID),
			},
			sdp.Attribute{
				Key:   "ssrc",
				Value: fmt.Sprintf("%d cname:%s", slot.SSRC, cname),
			},
			sdp.Attribute{
				Key:   "ssrc",
				Value: fmt.Sprintf("%d msid:%s %s", slot.SSRC, participantSID, slot.TrackSID),
			},
		)

		if slot.RTXSSRC > 0 {
			md.Attributes = append(md.Attributes,
				sdp.Attribute{
					Key:   "ssrc",
					Value: fmt.Sprintf("%d cname:%s", slot.RTXSSRC, cname),
				},
				sdp.Attribute{
					Key:   "ssrc-group",
					Value: fmt.Sprintf("FID %d %d", slot.SSRC, slot.RTXSSRC),
				},
			)
		}

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
