package livekit_test

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"ekbatan-room/signaling-coordinator/internal/adapters/inbound/livekit"
)

func TestWithCORS_PreflightOptions(t *testing.T) {
	dummy := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
	})

	handler := livekit.WithCORS(dummy)

	req := httptest.NewRequest(http.MethodOptions, "/rtc/v1/validate", nil)
	req.Header.Set("Origin", "http://localhost:3000")
	rec := httptest.NewRecorder()

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusNoContent {
		t.Fatalf("expected 204 No Content for OPTIONS, got %d", rec.Code)
	}
	if rec.Header().Get("Access-Control-Allow-Origin") != "http://localhost:3000" {
		t.Fatalf("expected Access-Control-Allow-Origin: http://localhost:3000, got %s", rec.Header().Get("Access-Control-Allow-Origin"))
	}
	if rec.Header().Get("Access-Control-Allow-Headers") != "*" {
		t.Fatalf("expected Access-Control-Allow-Headers: *, got %s", rec.Header().Get("Access-Control-Allow-Headers"))
	}
}

func TestValidateHandler_GET(t *testing.T) {
	valHandler := livekit.NewValidateHandler()

	req := httptest.NewRequest(http.MethodGet, "/rtc/validate", nil)
	req.Header.Set("Origin", "http://example.com")
	rec := httptest.NewRecorder()

	valHandler.ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("expected 200 OK, got %d", rec.Code)
	}
	if rec.Header().Get("Access-Control-Allow-Origin") != "http://example.com" {
		t.Fatalf("expected Access-Control-Allow-Origin: http://example.com, got %s", rec.Header().Get("Access-Control-Allow-Origin"))
	}

	var res map[string]interface{}
	if err := json.Unmarshal(rec.Body.Bytes(), &res); err != nil {
		t.Fatalf("failed to decode response JSON: %v", err)
	}
	if res["status"] != "success" || res["valid"] != true {
		t.Fatalf("unexpected response body: %+v", res)
	}
}
