package livekit

import (
	"encoding/json"
	"net/http"
)

// WithCORS wraps an http.Handler with Cross-Origin Resource Sharing (CORS) headers.
// It handles preflight OPTIONS requests with HTTP 204 No Content.
func WithCORS(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		origin := r.Header.Get("Origin")
		if origin == "" {
			origin = "*"
		}
		w.Header().Set("Access-Control-Allow-Origin", origin)
		w.Header().Set("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")
		w.Header().Set("Access-Control-Allow-Headers", "*")
		w.Header().Set("Access-Control-Max-Age", "86400")

		if r.Method == http.MethodOptions {
			w.WriteHeader(http.StatusNoContent)
			return
		}

		next.ServeHTTP(w, r)
	})
}

// ValidateHandler handles HTTP validation probes on /rtc/validate and /rtc/v1/validate (REQ-001).
// Standard browsers using livekit-client fetch this URL when validating connectivity or probing endpoints.
type ValidateHandler struct{}

// NewValidateHandler creates a new ValidateHandler instance.
func NewValidateHandler() *ValidateHandler {
	return &ValidateHandler{}
}

func (h *ValidateHandler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	origin := r.Header.Get("Origin")
	if origin == "" {
		origin = "*"
	}
	w.Header().Set("Access-Control-Allow-Origin", origin)
	w.Header().Set("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
	w.Header().Set("Access-Control-Allow-Headers", "*")
	w.Header().Set("Content-Type", "application/json")

	if r.Method == http.MethodOptions {
		w.WriteHeader(http.StatusNoContent)
		return
	}

	w.WriteHeader(http.StatusOK)
	_ = json.NewEncoder(w).Encode(map[string]interface{}{
		"status": "success",
		"valid":  true,
	})
}
