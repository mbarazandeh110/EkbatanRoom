import "@testing-library/jest-dom";

// Polyfill ResizeObserver and IntersectionObserver for jsdom
if (typeof window !== "undefined") {
  if (!window.ResizeObserver) {
    window.ResizeObserver = class ResizeObserver {
      observe() {}
      unobserve() {}
      disconnect() {}
    };
  }

  if (!window.IntersectionObserver) {
    window.IntersectionObserver = class IntersectionObserver {
      readonly root: Element | null = null;
      readonly rootMargin: string = "";
      readonly thresholds: ReadonlyArray<number> = [];
      observe() {}
      unobserve() {}
      disconnect() {}
      takeRecords() {
        return [];
      }
    };
  }

  const win = window as unknown as Record<string, unknown>;
  const glob = globalThis as unknown as Record<string, unknown>;

  if (typeof win.MediaStream === "undefined") {
    class MockMediaStream {
      private tracks: MediaStreamTrack[] = [];
      constructor(tracks: MediaStreamTrack[] = []) {
        this.tracks = [...tracks];
      }
      getTracks() {
        return this.tracks;
      }
      getVideoTracks() {
        return this.tracks.filter((t) => t.kind === "video");
      }
      getAudioTracks() {
        return this.tracks.filter((t) => t.kind === "audio");
      }
      addTrack(track: MediaStreamTrack) {
        this.tracks.push(track);
      }
      removeTrack(track: MediaStreamTrack) {
        this.tracks = this.tracks.filter((t) => t !== track);
      }
    }
    win.MediaStream = MockMediaStream;
    glob.MediaStream = MockMediaStream;
  }
}
