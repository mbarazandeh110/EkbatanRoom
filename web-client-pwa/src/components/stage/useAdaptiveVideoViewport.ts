import { useEffect, RefObject } from "react";
import { AdaptiveViewportManager } from "../../application/adaptive_viewport_manager";

export function useAdaptiveVideoViewport(
  containerRef: RefObject<HTMLElement | null>,
  trackId: string | undefined,
  viewportManager?: AdaptiveViewportManager | null
) {
  useEffect(() => {
    const el = containerRef.current;
    if (!el || !trackId || !viewportManager) return;

    let isIntersecting = true;
    let width = el.clientWidth || 320;
    let height = el.clientHeight || 240;

    const intersectionObserver = new IntersectionObserver(
      (entries) => {
        const entry = entries[0];
        if (entry) {
          isIntersecting = entry.isIntersecting;
          viewportManager.onViewportChange(trackId, isIntersecting, {
            width,
            height,
          });
        }
      },
      { threshold: 0.1 }
    );

    const resizeObserver = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (entry) {
        width = entry.contentRect.width;
        height = entry.contentRect.height;
        viewportManager.onViewportChange(trackId, isIntersecting, {
          width,
          height,
        });
      }
    });

    intersectionObserver.observe(el);
    resizeObserver.observe(el);

    // Initial evaluation
    viewportManager.onViewportChange(trackId, isIntersecting, { width, height });

    return () => {
      intersectionObserver.disconnect();
      resizeObserver.disconnect();
      viewportManager.unregisterTrack(trackId);
    };
  }, [containerRef, trackId, viewportManager]);
}
