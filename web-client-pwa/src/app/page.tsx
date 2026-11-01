"use client";

import React, { useState } from "react";
import { useRouter } from "next/navigation";
import { Video, ArrowRight, ShieldCheck, Zap } from "lucide-react";

export default function HomePage() {
  const router = useRouter();
  const [roomId, setRoomId] = useState("");

  const handleJoin = (e: React.FormEvent) => {
    e.preventDefault();
    if (!roomId.trim()) return;
    router.push(`/room/${encodeURIComponent(roomId.trim())}`);
  };

  const handleStartInstant = () => {
    const randomRoom = `room-${Math.random().toString(36).substring(2, 8)}`;
    router.push(`/room/${randomRoom}`);
  };

  return (
    <main className="min-h-screen w-full bg-slate-950 flex flex-col justify-center items-center px-4 py-12">
      <div className="max-w-md w-full space-y-8 text-center">
        <div>
          <div className="inline-flex p-3 rounded-2xl bg-emerald-500/10 border border-emerald-500/20 text-emerald-400 mb-4 shadow-lg shadow-emerald-500/5">
            <Video className="w-8 h-8" />
          </div>
          <h1 className="text-3xl font-extrabold text-white tracking-tight">
            EkbatanRoom
          </h1>
          <p className="mt-2 text-sm text-slate-400">
            Enterprise Next.js WebRTC Conference with Dual-Protocol Architecture
          </p>
        </div>

        <div className="bg-slate-900/80 backdrop-blur-xl border border-slate-800 p-6 rounded-2xl shadow-xl space-y-6">
          <form onSubmit={handleJoin} className="space-y-4">
            <div>
              <label htmlFor="room-id" className="block text-xs font-semibold text-slate-400 text-left mb-1.5 uppercase tracking-wider">
                Join with Room ID
              </label>
              <input
                id="room-id"
                type="text"
                placeholder="e.g. daily-sync"
                value={roomId}
                onChange={(e) => setRoomId(e.target.value)}
                className="w-full bg-slate-800 border border-slate-700 rounded-xl px-4 py-3 text-white placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-emerald-500 focus:border-transparent transition-all"
              />
            </div>
            <button
              type="submit"
              disabled={!roomId.trim()}
              className="w-full flex items-center justify-center space-x-2 bg-emerald-600 hover:bg-emerald-500 disabled:opacity-50 disabled:cursor-not-allowed text-white font-medium py-3 px-4 rounded-xl shadow-lg shadow-emerald-600/30 transition-all"
            >
              <span>Join Meeting</span>
              <ArrowRight className="w-4 h-4" />
            </button>
          </form>

          <div className="relative">
            <div className="absolute inset-0 flex items-center">
              <div className="w-full border-t border-slate-800" />
            </div>
            <div className="relative flex justify-center text-xs uppercase">
              <span className="bg-slate-900 px-2 text-slate-500 font-medium">Or</span>
            </div>
          </div>

          <button
            type="button"
            onClick={handleStartInstant}
            className="w-full flex items-center justify-center space-x-2 bg-slate-800 hover:bg-slate-700 text-slate-200 font-medium py-3 px-4 rounded-xl border border-slate-700 transition-all"
          >
            <Zap className="w-4 h-4 text-emerald-400" />
            <span>Create Instant Room</span>
          </button>
        </div>

        <div className="flex justify-center space-x-6 text-xs text-slate-500">
          <div className="flex items-center space-x-1.5">
            <ShieldCheck className="w-4 h-4 text-slate-400" />
            <span>E2EE Ready</span>
          </div>
          <div className="flex items-center space-x-1.5">
            <Zap className="w-4 h-4 text-slate-400" />
            <span>Adaptive Stream</span>
          </div>
        </div>
      </div>
    </main>
  );
}
