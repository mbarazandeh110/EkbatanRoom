package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"log/slog"
	"net/http"
	"os"
	"os/signal"
	"syscall"

	"go.opentelemetry.io/contrib/instrumentation/google.golang.org/grpc/otelgrpc"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"

	sfuv1 "ekbatan-room/signaling-coordinator/gen/proto/sfu/v1"
	livekitproto "github.com/livekit/protocol/livekit"
	"ekbatan-room/signaling-coordinator/internal/adapters/inbound/livekit"
	"ekbatan-room/signaling-coordinator/internal/adapters/inbound/native_ortc"
	"ekbatan-room/signaling-coordinator/internal/adapters/outbound/media_node_grpc"
	"ekbatan-room/signaling-coordinator/internal/adapters/outbound/telemetry"
	"ekbatan-room/signaling-coordinator/internal/application"
	"ekbatan-room/signaling-coordinator/internal/config"
)

var (
	version   = "dev"
	gitCommit = "none"
	buildTime = "unknown"
)

func main() {
	configFlag := flag.String("config", "", "Path to YAML configuration file (env: CONFIG_FILE)")
	portFlag := flag.String("port", "", "HTTP/WebSocket signaling server port (overrides config and PORT env)")
	sfuAddrFlag := flag.String("sfu-addr", "", "Target address of sfu-media-node gRPC server (overrides config and SFU_GRPC_ADDR env)")
	versionFlag := flag.Bool("version", false, "Print version information and exit")
	vFlag := flag.Bool("v", false, "Print version information and exit (shorthand)")

	flag.Usage = func() {
		fmt.Fprintf(os.Stderr, "EkbatanRoom Signaling Coordinator (%s)\n\n", version)
		fmt.Fprintf(os.Stderr, "Usage: %s [flags]\n\n", os.Args[0])
		fmt.Fprintf(os.Stderr, "Flags:\n")
		flag.PrintDefaults()
		fmt.Fprintf(os.Stderr, "\nConfiguration & Environment Variables:\n")
		fmt.Fprintf(os.Stderr, "  CONFIG_FILE              Path to YAML configuration file\n")
		fmt.Fprintf(os.Stderr, "  PORT                     Server listen port (default: 7880)\n")
		fmt.Fprintf(os.Stderr, "  SFU_GRPC_ADDR            Media node gRPC endpoint (default: 127.0.0.1:50051)\n")
		fmt.Fprintf(os.Stderr, "  SFU_REQUEST_TIMEOUT      Media node gRPC call timeout (default: 5s)\n")
		fmt.Fprintf(os.Stderr, "  LIVEKIT_PING_INTERVAL    Ping interval seconds (default: 10)\n")
		fmt.Fprintf(os.Stderr, "  LIVEKIT_PING_TIMEOUT     Ping timeout seconds (default: 20)\n")
		fmt.Fprintf(os.Stderr, "  LIVEKIT_SERVER_VERSION   Reported server version (default: 1.53.0)\n")
		fmt.Fprintf(os.Stderr, "  RTC_ICE_SERVERS          ICE servers (JSON array or comma-separated URLs)\n")
		fmt.Fprintf(os.Stderr, "  EKBATAN_LICENSE_KEY      Commercial Ed25519 license token\n")
		fmt.Fprintf(os.Stderr, "  EKBATAN_LICENSE_PUBLIC_KEY Public key hex override for license verification\n")
		fmt.Fprintf(os.Stderr, "  OBS_LOG_LEVEL            Logging level: debug, info, warn, error (default: info)\n")
		fmt.Fprintf(os.Stderr, "  OBS_LOG_FORMAT           Logging format: json, text (default: json)\n")
		fmt.Fprintf(os.Stderr, "  OBS_LOG_RAW_SDP          Log raw SDP offer/answer if level=debug (default: false)\n")
		fmt.Fprintf(os.Stderr, "  OBS_TRACING_ENABLED      Enable OpenTelemetry tracing (default: false)\n")
		fmt.Fprintf(os.Stderr, "  OBS_TRACE_SAMPLE_RATE    Trace sampling ratio 0.0-1.0 (default: 0.1)\n")
		fmt.Fprintf(os.Stderr, "  OBS_OTLP_ENDPOINT        OpenTelemetry gRPC collector endpoint\n")
		fmt.Fprintf(os.Stderr, "  OBS_METRICS_ENABLED      Enable /metrics endpoint (default: true)\n")
		fmt.Fprintf(os.Stderr, "  OBS_METRICS_ADDR         Separate HTTP address for Prometheus metrics (optional)\n\n")
	}

	flag.Parse()

	if *versionFlag || *vFlag {
		fmt.Printf("signaling-coordinator %s (commit: %s, built: %s)\n", version, gitCommit, buildTime)
		return
	}

	// 1. Load Configuration (defaults -> YAML -> ENV)
	cfg, err := config.Load(*configFlag)
	if err != nil {
		fmt.Fprintf(os.Stderr, "fatal: failed to load configuration: %v\n", err)
		os.Exit(1)
	}

	// Override from CLI flags if explicitly specified
	if *portFlag != "" {
		cfg.Server.Port = *portFlag
	}
	if *sfuAddrFlag != "" {
		cfg.SFU.GRPCAddr = *sfuAddrFlag
	}

	// 2. Initialize Observability (Logging & Tracing)
	_ = telemetry.SetupLogger(cfg.Telemetry, nil)

	ctx := context.Background()
	shutdownTracer, err := telemetry.SetupTracer(ctx, cfg.Telemetry)
	if err != nil {
		slog.Warn("failed to initialize tracer provider", "error", err)
	}
	defer shutdownTracer(context.Background())

	slog.Info("starting signaling-coordinator",
		"port", cfg.Server.Port,
		"sfu_grpc_addr", cfg.SFU.GRPCAddr,
		"sfu_timeout", cfg.SFU.RequestTimeout.String(),
		"ping_interval", cfg.LiveKit.PingInterval,
		"ping_timeout", cfg.LiveKit.PingTimeout,
		"log_level", cfg.Telemetry.LogLevel,
		"tracing_enabled", cfg.Telemetry.TracingEnabled,
		"metrics_enabled", cfg.Telemetry.MetricsEnabled,
	)

	// 3. Establish gRPC connection to SFU Media Node with OpenTelemetry propagation
	conn, err := grpc.NewClient(
		cfg.SFU.GRPCAddr,
		grpc.WithTransportCredentials(insecure.NewCredentials()),
		grpc.WithStatsHandler(otelgrpc.NewClientHandler()),
	)
	if err != nil {
		slog.Error("failed to dial SFU media node", "error", err, "sfu_addr", cfg.SFU.GRPCAddr)
		os.Exit(1)
	}
	defer conn.Close()

	sfuClient := sfuv1.NewSfuMediaEngineServiceClient(conn)
	mediaNodeAdapter := media_node_grpc.NewMediaNodeClient(sfuClient, cfg.SFU.RequestTimeout)

	// 4. Initialize License Manager & Node Registry
	licenseMgr, err := application.NewLicenseManager(cfg.License.Key, cfg.License.PublicKey)
	if err != nil {
		slog.Error("failed to initialize license manager", "error", err)
		os.Exit(1)
	}

	nodeRegistry := application.NewNodeRegistry("coordinator-main", licenseMgr)
	if _, err := nodeRegistry.RegisterNode(context.Background(), "sfu-primary", mediaNodeAdapter); err != nil {
		slog.Warn("could not establish event stream with SFU media node at startup", "error", err)
	}

	// 5. Assemble Application layer with Constructor DI
	roomManager := application.NewRoomManager(
		mediaNodeAdapter,
		mediaNodeAdapter,
		mediaNodeAdapter,
		cfg.RTC.IceServers,
		licenseMgr,
	)
	roomManager.SetMetricsPort(telemetry.DefaultMetrics())
	roomManager.SetNodeRegistry(nodeRegistry)

	// 6. Assemble Inbound Adapters (Dual Gateway)
	liveKitHandler := livekit.NewLiveKitWSHandler(roomManager, livekit.ResponderConfig{
		PingInterval:  cfg.LiveKit.PingInterval,
		PingTimeout:   cfg.LiveKit.PingTimeout,
		ServerVersion: cfg.LiveKit.ServerVersion,
	})
	ortcHandler := native_ortc.NewNativeOrtcHandler(roomManager)

	mux := http.NewServeMux()
	mux.Handle("/rtc", liveKitHandler)
	// NOTE (REQ-008): /rtc/ is intentionally NOT registered so that /rtc/v1 returns HTTP 404,
	// triggering official livekit-client protocol fallback to standard /rtc WebSocket signaling.
	valHandler := livekit.NewValidateHandler()
	mux.Handle("/rtc/validate", valHandler)
	mux.Handle("/rtc/v1/validate", valHandler)
	mux.Handle("/ws/ortc", ortcHandler)

	// LiveKit Twirp RoomService API (/twirp/livekit.RoomService/*)
	twirpRoomSvc := livekit.NewRoomServiceServer(roomManager)
	twirpHandler := livekitproto.NewRoomServiceServer(twirpRoomSvc)
	mux.Handle(twirpHandler.PathPrefix(), twirpHandler)
	mux.Handle("/twirp/", twirpHandler)

	var metricsSrv *http.Server
	if cfg.Telemetry.MetricsEnabled {
		if cfg.Telemetry.MetricsAddr != "" {
			metricsMux := http.NewServeMux()
			metricsMux.Handle("/metrics", telemetry.DefaultMetrics().Handler())
			metricsSrv = &http.Server{
				Addr:    cfg.Telemetry.MetricsAddr,
				Handler: livekit.WithCORS(metricsMux),
			}
			go func() {
				slog.Info("dedicated metrics server listening", "addr", cfg.Telemetry.MetricsAddr)
				if err := metricsSrv.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
					slog.Error("metrics server failure", "error", err)
				}
			}()
		} else {
			mux.Handle("/metrics", telemetry.DefaultMetrics().Handler())
		}
	}
	mux.HandleFunc("/healthz", func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte("ok"))
	})

	srv := &http.Server{
		Addr:              fmt.Sprintf(":%s", cfg.Server.Port),
		Handler:           livekit.WithCORS(mux),
		ReadHeaderTimeout: cfg.Server.ReadTimeout,
	}

	// 7. Graceful shutdown handler
	stopChan := make(chan os.Signal, 1)
	signal.Notify(stopChan, os.Interrupt, syscall.SIGTERM)

	go func() {
		if err := srv.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
			slog.Error("HTTP server failure", "error", err)
			os.Exit(1)
		}
	}()

	slog.Info("server listening", "port", cfg.Server.Port)
	<-stopChan
	slog.Info("shutting down gracefully...")

	shutdownCtx, cancel := context.WithTimeout(context.Background(), cfg.Server.ShutdownTimeout)
	defer cancel()

	if metricsSrv != nil {
		_ = metricsSrv.Shutdown(shutdownCtx)
	}

	if err := srv.Shutdown(shutdownCtx); err != nil {
		slog.Error("HTTP server shutdown error", "error", err)
	}

	// Release SFU media routers and transports for all active rooms
	if err := roomManager.Close(shutdownCtx); err != nil {
		slog.Error("error releasing room SFU resources", "error", err)
	}

	nodeRegistry.Close()
	slog.Info("shutdown complete")
}
