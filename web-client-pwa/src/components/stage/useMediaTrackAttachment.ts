import { useEffect, RefObject } from "react";

export function useMediaTrackAttachment(
  videoRef: RefObject<HTMLVideoElement | null>,
  track?: MediaStreamTrack | null
) {
  useEffect(() => {
    const videoElement = videoRef.current;
    if (!videoElement) return;

    if (!track) {
      if (videoElement.srcObject) {
        videoElement.srcObject = null;
      }
      return;
    }

    const stream = new MediaStream([track]);
    videoElement.srcObject = stream;
    videoElement.play().catch(() => {
      // Autoplay might be temporarily restricted before user gesture
    });

    return () => {
      if (videoElement.srcObject) {
        videoElement.srcObject = null;
      }
    };
  }, [videoRef, track]);
}
