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
	"time"

	"go.opentelemetry.io/contrib/instrumentation/google.golang.org/grpc/otelgrpc"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"

	sfuv1 "ekbatan-room/signaling-coordinator/gen/proto/sfu/v1"
	"ekbatan-room/signaling-coordinator/internal/adapters/inbound/livekit"
	"ekbatan-room/signaling-coordinator/internal/adapters/inbound/native_ortc"
	"ekbatan-room/signaling-coordinator/internal/adapters/outbound/media_node_grpc"
	"ekbatan-room/signaling-coordinator/internal/adapters/outbound/telemetry"
	"ekbatan-room/signaling-coordinator/internal/application"
	"ekbatan-room/signaling-coordinator/internal/domain"
)

var (
	version   = "dev"
	gitCommit = "none"
	buildTime = "unknown"
)

func main() {
	defaultPort := os.Getenv("PORT")
	if defaultPort == "" {
		defaultPort = "7880"
	}

	defaultSfuAddr := os.Getenv("SFU_GRPC_ADDR")
	if defaultSfuAddr == "" {
		defaultSfuAddr = "127.0.0.1:50051"
	}

	portFlag := flag.String("port", defaultPort, "HTTP/WebSocket signaling server port (env: PORT)")
	sfuAddrFlag := flag.String("sfu-addr", defaultSfuAddr, "Target address of sfu-media-node gRPC server (env: SFU_GRPC_ADDR)")
	versionFlag := flag.Bool("version", false, "Print version information and exit")
	vFlag := flag.Bool("v", false, "Print version information and exit (shorthand)")

	flag.Usage = func() {
		fmt.Fprintf(os.Stderr, "EkbatanRoom Signaling Coordinator (%s)\n\n", version)
		fmt.Fprintf(os.Stderr, "Usage: %s [flags]\n\n", os.Args[0])
		fmt.Fprintf(os.Stderr, "Flags:\n")
		flag.PrintDefaults()
		fmt.Fprintf(os.Stderr, "\nEnvironment Variables:\n")
		fmt.Fprintf(os.Stderr, "  PORT                     Server listen port (default: 7880)\n")
		fmt.Fprintf(os.Stderr, "  SFU_GRPC_ADDR            Media node gRPC endpoint (default: 127.0.0.1:50051)\n")
		fmt.Fprintf(os.Stderr, "  OBS_LOG_LEVEL            Logging level: debug, info, warn, error (default: info)\n")
		fmt.Fprintf(os.Stderr, "  OBS_LOG_FORMAT           Logging format: json, text (default: json)\n")
		fmt.Fprintf(os.Stderr, "  OBS_LOG_RAW_SDP          Log raw SDP offer/answer if level=debug (default: false)\n")
		fmt.Fprintf(os.Stderr, "  OBS_TRACING_ENABLED      Enable OpenTelemetry tracing (default: false)\n")
		fmt.Fprintf(os.Stderr, "  OBS_TRACE_SAMPLE_RATE    Trace sampling ratio 0.0-1.0 (default: 1.0)\n")
		fmt.Fprintf(os.Stderr, "  OBS_OTLP_ENDPOINT        OpenTelemetry gRPC collector endpoint\n")
		fmt.Fprintf(os.Stderr, "  OBS_METRICS_ENABLED      Enable /metrics endpoint (default: true)\n\n")
	}

	flag.Parse()

	if *versionFlag || *vFlag {
		fmt.Printf("signaling-coordinator %s (commit: %s, built: %s)\n", version, gitCommit, buildTime)
		return
	}

	port := *portFlag
	sfuAddr := *sfuAddrFlag

	// 1. Initialize Observability (Logging & Tracing)
	telemetryCfg := telemetry.LoadConfigFromEnv()
	_ = telemetry.SetupLogger(telemetryCfg, nil)

	ctx := context.Background()
	shutdownTracer, err := telemetry.SetupTracer(ctx, telemetryCfg)
	if err != nil {
		slog.Warn("failed to initialize tracer provider", "error", err)
	}
	defer shutdownTracer(context.Background())

	slog.Info("starting signaling-coordinator",
		"port", port,
		"sfu_grpc_addr", sfuAddr,
		"log_level", telemetryCfg.LogLevel,
		"tracing_enabled", telemetryCfg.TracingEnabled,
		"metrics_enabled", telemetryCfg.MetricsEnabled,
	)

	// 2. Establish gRPC connection to SFU Media Node with OpenTelemetry propagation
	conn, err := grpc.NewClient(
		sfuAddr,
		grpc.WithTransportCredentials(insecure.NewCredentials()),
		grpc.WithStatsHandler(otelgrpc.NewClientHandler()),
	)
	if err != nil {
		slog.Error("failed to dial SFU media node", "error", err, "sfu_addr", sfuAddr)
		os.Exit(1)
	}
	defer conn.Close()

	sfuClient := sfuv1.NewSfuMediaEngineServiceClient(conn)
	mediaNodeAdapter := media_node_grpc.NewMediaNodeClient(sfuClient, 5*time.Second)

	// 3. Configure ICE servers
	iceServers := []domain.IceServer{
		{
			URLs: []string{"stun:stun.l.google.com:19302"},
		},
	}

	// 4. Assemble Application layer with Constructor DI
	roomManager := application.NewRoomManager(
		mediaNodeAdapter,
		mediaNodeAdapter,
		mediaNodeAdapter,
		iceServers,
	)

	// 5. Assemble Inbound Adapters (Dual Gateway)
	liveKitHandler := livekit.NewLiveKitWSHandler(roomManager)
	ortcHandler := native_ortc.NewNativeOrtcHandler(roomManager)

	mux := http.NewServeMux()
	mux.Handle("/rtc", liveKitHandler)
	mux.Handle("/rtc/", liveKitHandler)
	mux.Handle("/ws/ortc", ortcHandler)
	mux.Handle("/metrics", telemetry.DefaultMetrics().Handler())
	mux.HandleFunc("/healthz", func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte("ok"))
	})

	srv := &http.Server{
		Addr:         fmt.Sprintf(":%s", port),
		Handler:      mux,
		ReadTimeout:  15 * time.Second,
		WriteTimeout: 15 * time.Second,
	}

	// 6. Graceful shutdown handler
	stopChan := make(chan os.Signal, 1)
	signal.Notify(stopChan, os.Interrupt, syscall.SIGTERM)

	go func() {
		if err := srv.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
			slog.Error("HTTP server failure", "error", err)
			os.Exit(1)
		}
	}()

	slog.Info("server listening", "port", port)
	<-stopChan
	slog.Info("shutting down gracefully...")

	shutdownCtx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	if err := srv.Shutdown(shutdownCtx); err != nil {
		slog.Error("HTTP server shutdown error", "error", err)
	}
	slog.Info("shutdown complete")
}
