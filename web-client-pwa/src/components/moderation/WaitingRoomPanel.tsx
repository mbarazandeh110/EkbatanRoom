"use client";

import React from "react";
import { Users, Check, X, CheckCheck, Clock, ShieldCheck } from "lucide-react";

export interface PendingGuest {
  id: string;
  name: string;
  identity: string;
  requestedAt: number;
}

export interface WaitingRoomPanelProps {
  isWaitingRoomEnabled: boolean;
  onToggleWaitingRoom: (enabled: boolean) => void;
  pendingGuests: PendingGuest[];
  onAdmit: (guestId: string) => void;
  onAdmitAll: () => void;
  onReject: (guestId: string) => void;
}

export const WaitingRoomPanel: React.FC<WaitingRoomPanelProps> = ({
  isWaitingRoomEnabled,
  onToggleWaitingRoom,
  pendingGuests,
  onAdmit,
  onAdmitAll,
  onReject,
}) => {
  return (
    <div
      data-testid="waiting-room-panel"
      className="flex flex-col h-full w-full bg-slate-900 border border-slate-800 rounded-xl overflow-hidden shadow-2xl"
    >
      {/* Header with Master Toggle */}
      <div className="flex items-center justify-between px-4 py-3 bg-slate-950 border-b border-slate-800">
        <div className="flex items-center space-x-2">
          <ShieldCheck className="w-4 h-4 text-emerald-400" />
          <h2 className="text-sm font-semibold text-white">Waiting Room Admission</h2>
        </div>

        <button
          type="button"
          onClick={() => onToggleWaitingRoom(!isWaitingRoomEnabled)}
          aria-label={isWaitingRoomEnabled ? "Disable waiting room" : "Enable waiting room"}
          className={`px-2.5 py-1 rounded-lg text-xs font-medium border transition-colors ${
            isWaitingRoomEnabled
              ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/30"
              : "bg-slate-800 text-slate-400 border-slate-700"
          }`}
        >
          {isWaitingRoomEnabled ? "Active: ON" : "Active: OFF"}
        </button>
      </div>

      {/* Action Bar */}
      <div className="flex items-center justify-between px-4 py-2 bg-slate-900/60 border-b border-slate-800 text-xs">
        <span className="text-slate-400">
          Pending Attendees: <strong className="text-white font-mono">{pendingGuests.length}</strong>
        </span>

        {pendingGuests.length > 0 && (
          <button
            type="button"
            onClick={onAdmitAll}
            className="px-2.5 py-1 bg-emerald-600 hover:bg-emerald-500 text-white rounded-lg transition-colors flex items-center space-x-1 font-medium"
          >
            <CheckCheck className="w-3.5 h-3.5" />
            <span>Admit All</span>
          </button>
        )}
      </div>

      {/* Guest List */}
      <div className="flex-1 overflow-y-auto p-3 space-y-2">
        {pendingGuests.length === 0 ? (
          <div className="h-full flex flex-col items-center justify-center text-slate-500 text-xs py-8">
            <Users className="w-8 h-8 text-slate-700 mb-2" />
            <p>No attendees currently waiting for approval.</p>
          </div>
        ) : (
          pendingGuests.map((guest) => (
            <div
              key={guest.id}
              className="flex items-center justify-between p-3 rounded-lg bg-slate-800/60 border border-slate-700/60 hover:bg-slate-800 transition-colors"
            >
              <div className="min-w-0 pr-2">
                <p className="text-xs font-semibold text-slate-200 truncate">{guest.name}</p>
                <div className="flex items-center space-x-1 text-[10px] text-slate-400 mt-0.5">
                  <Clock className="w-3 h-3 text-slate-500" />
                  <span>
                    Waiting {Math.max(1, Math.round((Date.now() - guest.requestedAt) / 1000))}s
                  </span>
                </div>
              </div>

              <div className="flex items-center space-x-1.5 shrink-0">
                <button
                  type="button"
                  onClick={() => onAdmit(guest.id)}
                  aria-label={`Admit ${guest.name}`}
                  className="p-1.5 rounded-lg bg-emerald-600 hover:bg-emerald-500 text-white transition-colors"
                >
                  <Check className="w-4 h-4" />
                </button>
                <button
                  type="button"
                  onClick={() => onReject(guest.id)}
                  aria-label={`Reject ${guest.name}`}
                  className="p-1.5 rounded-lg bg-red-600/20 text-red-400 hover:bg-red-600 hover:text-white transition-colors"
                >
                  <X className="w-4 h-4" />
                </button>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
};
