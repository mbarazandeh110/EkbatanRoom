"use client";

import React, { useState, useEffect } from "react";
import { Lock, MicOff, VideoOff, MessageSquareOff, MessagesSquare, Edit3, Disc, X, Check } from "lucide-react";

export interface RoomLockSettings {
  lockAllMics: boolean;
  lockAllCameras: boolean;
  lockPublicChat: boolean;
  lockPrivateChat: boolean;
  lockWhiteboard: boolean;
  lockLocalRecording: boolean;
}

export interface RoomLockModalProps {
  isOpen: boolean;
  onClose: () => void;
  initialSettings?: Partial<RoomLockSettings>;
  onApplySettings: (settings: RoomLockSettings) => Promise<void> | void;
  onSendData?: (data: Uint8Array, topic: string) => Promise<void>;
}

export const RoomLockModal: React.FC<RoomLockModalProps> = ({
  isOpen,
  onClose,
  initialSettings,
  onApplySettings,
  onSendData,
}) => {
  const [settings, setSettings] = useState<RoomLockSettings>({
    lockAllMics: initialSettings?.lockAllMics ?? false,
    lockAllCameras: initialSettings?.lockAllCameras ?? false,
    lockPublicChat: initialSettings?.lockPublicChat ?? false,
    lockPrivateChat: initialSettings?.lockPrivateChat ?? false,
    lockWhiteboard: initialSettings?.lockWhiteboard ?? false,
    lockLocalRecording: initialSettings?.lockLocalRecording ?? false,
  });

  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    if (initialSettings) {
      setSettings((prev) => ({ ...prev, ...initialSettings }));
    }
  }, [initialSettings]);

  // Keyboard Escape
  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  const toggleSetting = (key: keyof RoomLockSettings) => {
    setSettings((prev) => ({ ...prev, [key]: !prev[key] }));
  };

  const handleSave = async () => {
    setIsSaving(true);
    try {
      await onApplySettings(settings);
      if (onSendData) {
        const payload = new TextEncoder().encode(JSON.stringify(settings));
        await onSendData(payload, "room_lock_settings");
      }
      onClose();
    } finally {
      setIsSaving(false);
    }
  };

  const lockOptions: {
    key: keyof RoomLockSettings;
    title: string;
    description: string;
    icon: React.ComponentType<{ className?: string }>;
  }[] = [
    {
      key: "lockAllMics",
      title: "Mute All Microphones",
      description: "Force mute all attendees and prevent unmuting without permission",
      icon: MicOff,
    },
    {
      key: "lockAllCameras",
      title: "Disable All Cameras",
      description: "Prevent non-host participants from broadcasting video tracks",
      icon: VideoOff,
    },
    {
      key: "lockPublicChat",
      title: "Lock Public Chat",
      description: "Disallow participants from posting messages to the public room chat",
      icon: MessageSquareOff,
    },
    {
      key: "lockPrivateChat",
      title: "Lock Private Messages",
      description: "Disallow 1:1 direct messaging between attendees",
      icon: MessagesSquare,
    },
    {
      key: "lockWhiteboard",
      title: "Lock Collaborative Whiteboard",
      description: "Allow only hosts/instructors to draw and navigate slides",
      icon: Edit3,
    },
    {
      key: "lockLocalRecording",
      title: "Prohibit Local Recording",
      description: "Prevent students and viewers from recording session to local disk",
      icon: Disc,
    },
  ];

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="lock-modal-title"
      data-testid="room-lock-modal"
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-slate-950/80 backdrop-blur-sm animate-in fade-in duration-150"
    >
      <div className="w-full max-w-lg bg-slate-900 border border-slate-800 rounded-2xl shadow-2xl overflow-hidden flex flex-col">
        {/* Modal Header */}
        <div className="flex items-center justify-between px-5 py-4 bg-slate-950 border-b border-slate-800">
          <div className="flex items-center space-x-2">
            <div className="p-2 rounded-lg bg-red-500/10 text-red-400">
              <Lock className="w-4 h-4" />
            </div>
            <div>
              <h2 id="lock-modal-title" className="text-sm font-semibold text-white">
                Room Lock & Permission Settings
              </h2>
              <p className="text-xs text-slate-400">Granular moderation controls for this meeting</p>
            </div>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close lock settings modal"
            className="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Lock Toggles List */}
        <div className="p-5 space-y-3 overflow-y-auto max-h-[60vh]">
          {lockOptions.map((opt) => {
            const isLocked = settings[opt.key];
            const IconComponent = opt.icon;

            return (
              <div
                key={opt.key}
                onClick={() => toggleSetting(opt.key)}
                className={`p-3.5 rounded-xl border transition-all cursor-pointer flex items-center justify-between select-none ${
                  isLocked
                    ? "bg-red-950/20 border-red-500/40 text-white"
                    : "bg-slate-800/40 border-slate-800 hover:border-slate-700 text-slate-300"
                }`}
              >
                <div className="flex items-center space-x-3 pr-3">
                  <div
                    className={`p-2 rounded-lg shrink-0 ${
                      isLocked ? "bg-red-500/20 text-red-400" : "bg-slate-800 text-slate-400"
                    }`}
                  >
                    <IconComponent className="w-4 h-4" />
                  </div>
                  <div>
                    <h3 className="text-xs font-semibold text-slate-200">{opt.title}</h3>
                    <p className="text-[11px] text-slate-400 leading-tight mt-0.5">{opt.description}</p>
                  </div>
                </div>

                <div
                  className={`w-9 h-5 rounded-full p-0.5 transition-colors shrink-0 flex items-center ${
                    isLocked ? "bg-red-600 justify-end" : "bg-slate-700 justify-start"
                  }`}
                >
                  <div className="w-4 h-4 rounded-full bg-white shadow-sm" />
                </div>
              </div>
            );
          })}
        </div>

        {/* Modal Actions */}
        <div className="flex items-center justify-end space-x-2 px-5 py-3.5 bg-slate-950 border-t border-slate-800 text-xs">
          <button
            type="button"
            onClick={onClose}
            className="px-4 py-2 rounded-lg text-slate-300 hover:bg-slate-800 transition-colors"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={handleSave}
            disabled={isSaving}
            className="px-4 py-2 bg-red-600 hover:bg-red-500 disabled:opacity-50 text-white font-medium rounded-lg shadow-lg shadow-red-600/30 transition-all flex items-center space-x-1.5"
          >
            <Check className="w-3.5 h-3.5" />
            <span>Apply Restrictions</span>
          </button>
        </div>
      </div>
    </div>
  );
};
