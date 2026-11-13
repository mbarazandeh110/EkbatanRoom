"use client";

import React, { useRef, useState, useEffect, useCallback } from "react";
import { Play, Pause, Volume2, VolumeX, Link as LinkIcon, RefreshCw } from "lucide-react";

export interface MediaSyncPayload {
  action: "play" | "pause" | "seek" | "load";
  currentTime: number;
  mediaUrl: string;
  hostTimestamp: number;
}

export interface SynchronizedMediaPlayerProps {
  isHost?: boolean;
  initialMediaUrl?: string;
  onSendData?: (data: Uint8Array, topic: string) => Promise<void>;
  incomingData?: { data: Uint8Array; topic: string } | null;
}

export const SynchronizedMediaPlayer: React.FC<SynchronizedMediaPlayerProps> = ({
  isHost = true,
  initialMediaUrl = "",
  onSendData,
  incomingData,
}) => {
  const videoRef = useRef<HTMLVideoElement>(null);
  const [mediaUrl, setMediaUrl] = useState<string>(initialMediaUrl);
  const [inputUrl, setInputUrl] = useState<string>(initialMediaUrl);
  const [isPlaying, setIsPlaying] = useState<boolean>(false);
  const [currentTime, setCurrentTime] = useState<number>(0);
  const [duration, setDuration] = useState<number>(0);
  const [isMuted, setIsMuted] = useState<boolean>(false);
  const [volume, setVolume] = useState<number>(1);
  const isUpdatingFromRemote = useRef<boolean>(false);

  // Broadcast sync payload
  const broadcastSync = useCallback(
    async (action: MediaSyncPayload["action"], time: number, url: string) => {
      if (!isHost || !onSendData) return;
      try {
        const payload: MediaSyncPayload = {
          action,
          currentTime: time,
          mediaUrl: url,
          hostTimestamp: Date.now(),
        };
        const bytes = new TextEncoder().encode(JSON.stringify(payload));
        await onSendData(bytes, "media_sync");
      } catch {}
    },
    [isHost, onSendData]
  );

  // Process incoming remote sync messages
  useEffect(() => {
    if (!incomingData || incomingData.topic !== "media_sync") return;
    try {
      const text = new TextDecoder().decode(incomingData.data);
      const sync = JSON.parse(text) as MediaSyncPayload;
      const video = videoRef.current;
      if (!video) return;

      isUpdatingFromRemote.current = true;

      if (sync.mediaUrl && sync.mediaUrl !== mediaUrl) {
        setMediaUrl(sync.mediaUrl);
        video.src = sync.mediaUrl;
      }

      // Drift correction: if viewer is off by more than 1.5s
      const latencySeconds = (Date.now() - sync.hostTimestamp) / 1000;
      const estimatedHostTime = sync.currentTime + (sync.action === "play" ? latencySeconds : 0);

      if (Math.abs(video.currentTime - estimatedHostTime) > 1.5) {
        video.currentTime = Math.max(0, estimatedHostTime);
      }

      if (sync.action === "play") {
        video.play().catch(() => {});
        setIsPlaying(true);
      } else if (sync.action === "pause") {
        video.pause();
        setIsPlaying(false);
      } else if (sync.action === "seek") {
        video.currentTime = sync.currentTime;
      }

      setTimeout(() => {
        isUpdatingFromRemote.current = false;
      }, 200);
    } catch {}
  }, [incomingData, mediaUrl]);

  const handleLoadUrl = (e: React.FormEvent) => {
    e.preventDefault();
    if (!inputUrl.trim() || !isHost) return;
    setMediaUrl(inputUrl.trim());
    if (videoRef.current) {
      videoRef.current.src = inputUrl.trim();
      videoRef.current.load();
    }
    broadcastSync("load", 0, inputUrl.trim());
  };

  const handlePlayPause = () => {
    const video = videoRef.current;
    if (!video) return;

    if (video.paused) {
      video.play().catch(() => {});
      setIsPlaying(true);
      broadcastSync("play", video.currentTime, mediaUrl);
    } else {
      video.pause();
      setIsPlaying(false);
      broadcastSync("pause", video.currentTime, mediaUrl);
    }
  };

  const handleSeek = (e: React.ChangeEvent<HTMLInputElement>) => {
    const newTime = Number(e.target.value);
    const video = videoRef.current;
    if (!video) return;
    video.currentTime = newTime;
    setCurrentTime(newTime);
    broadcastSync("seek", newTime, mediaUrl);
  };

  const handleTimeUpdate = () => {
    if (videoRef.current) {
      setCurrentTime(videoRef.current.currentTime);
    }
  };

  const handleLoadedMetadata = () => {
    if (videoRef.current) {
      setDuration(videoRef.current.duration || 0);
    }
  };

  const formatTime = (seconds: number) => {
    const m = Math.floor(seconds / 60);
    const s = Math.floor(seconds % 60);
    return `${m}:${s < 10 ? "0" : ""}${s}`;
  };

  return (
    <div
      data-testid="sync-media-player"
      className="flex flex-col h-full w-full bg-slate-900 border border-slate-800 rounded-xl overflow-hidden shadow-2xl"
    >
      {/* Host Media URL Loader */}
      {isHost && (
        <form
          onSubmit={handleLoadUrl}
          className="flex items-center space-x-2 p-3 bg-slate-950 border-b border-slate-800"
        >
          <div className="relative flex-1">
            <LinkIcon className="absolute left-3 top-2.5 w-4 h-4 text-slate-500" />
            <input
              type="url"
              placeholder="Paste direct MP4/WebM video URL..."
              value={inputUrl}
              onChange={(e) => setInputUrl(e.target.value)}
              className="w-full pl-9 pr-3 py-1.5 bg-slate-800 border border-slate-700 rounded-lg text-xs text-white placeholder-slate-500 focus:outline-none focus:ring-1 focus:ring-emerald-500"
            />
          </div>
          <button
            type="submit"
            className="px-3 py-1.5 bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-medium rounded-lg transition-colors flex items-center space-x-1"
          >
            <RefreshCw className="w-3.5 h-3.5" />
            <span>Load Media</span>
          </button>
        </form>
      )}

      {/* Video Viewport */}
      <div className="flex-1 relative bg-black flex items-center justify-center overflow-hidden min-h-0">
        <video
          ref={videoRef}
          src={mediaUrl || undefined}
          playsInline
          onTimeUpdate={handleTimeUpdate}
          onLoadedMetadata={handleLoadedMetadata}
          className="w-full h-full object-contain"
        />

        {!mediaUrl && (
          <div className="text-slate-500 text-xs flex flex-col items-center space-y-2">
            <p>No media currently loaded.</p>
            <span className="text-[10px] text-slate-600">
              {isHost ? "Enter a media URL above to stream together." : "Waiting for host to play media..."}
            </span>
          </div>
        )}
      </div>

      {/* Custom Synchronized Controls */}
      <div className="flex items-center justify-between px-4 py-3 bg-slate-950 border-t border-slate-800 text-xs">
        {/* Play/Pause */}
        <div className="flex items-center space-x-3">
          {isHost ? (
            <button
              type="button"
              onClick={handlePlayPause}
              disabled={!mediaUrl}
              aria-label={isPlaying ? "Pause shared media" : "Play shared media"}
              className="p-2 rounded-full bg-emerald-600 hover:bg-emerald-500 disabled:opacity-40 text-white transition-colors"
            >
              {isPlaying ? <Pause className="w-4 h-4" /> : <Play className="w-4 h-4" />}
            </button>
          ) : (
            <span className="text-[11px] text-slate-400 font-medium">
              {isPlaying ? "Playing (Host Sync)" : "Paused (Host Sync)"}
            </span>
          )}

          {/* Time text */}
          <span className="text-slate-400 font-mono text-[11px]">
            {formatTime(currentTime)} / {formatTime(duration)}
          </span>
        </div>

        {/* Timeline Slider (Host scrubber) */}
        <div className="flex-1 mx-4">
          <input
            type="range"
            min={0}
            max={duration || 100}
            value={currentTime}
            disabled={!isHost || !mediaUrl}
            onChange={handleSeek}
            aria-label="Media timeline scrub"
            className="w-full h-1 bg-slate-700 rounded-lg appearance-none cursor-pointer disabled:cursor-not-allowed accent-emerald-500"
          />
        </div>

        {/* Local Volume / Mute (Viewer Independent) */}
        <div className="flex items-center space-x-2">
          <button
            type="button"
            onClick={() => {
              const nextMuted = !isMuted;
              setIsMuted(nextMuted);
              if (videoRef.current) videoRef.current.muted = nextMuted;
            }}
            aria-label={isMuted ? "Unmute local media audio" : "Mute local media audio"}
            className="p-1.5 text-slate-400 hover:text-white"
          >
            {isMuted ? <VolumeX className="w-4 h-4" /> : <Volume2 className="w-4 h-4" />}
          </button>
          <input
            type="range"
            min={0}
            max={1}
            step={0.05}
            value={isMuted ? 0 : volume}
            onChange={(e) => {
              const val = Number(e.target.value);
              setVolume(val);
              setIsMuted(false);
              if (videoRef.current) {
                videoRef.current.volume = val;
                videoRef.current.muted = false;
              }
            }}
            aria-label="Local audio volume"
            className="w-14 h-1 bg-slate-700 rounded-lg appearance-none cursor-pointer accent-emerald-500"
          />
        </div>
      </div>
    </div>
  );
};
