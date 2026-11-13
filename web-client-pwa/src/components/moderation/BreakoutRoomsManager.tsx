"use client";

import React, { useState, useEffect, useCallback } from "react";
import {
  Split,
  Plus,
  Play,
  Square,
  Megaphone,
  ArrowRightLeft,
  Clock,
  Shuffle,
  Send,
  DoorOpen,
} from "lucide-react";

export interface BreakoutSubRoom {
  id: string;
  name: string;
  participantIds: string[];
}

export interface BreakoutState {
  isActive: boolean;
  rooms: BreakoutSubRoom[];
  timerSecondsRemaining: number;
  selfSelectAllowed: boolean;
  announcement?: string;
}

export interface BreakoutRoomsManagerProps {
  isHost?: boolean;
  allParticipants: { id: string; name: string }[];
  currentParticipantId: string;
  onJoinSubRoom?: (subRoomId: string) => void;
  onReturnToMainRoom?: () => void;
  onSendData?: (data: Uint8Array, topic: string) => Promise<void>;
  incomingData?: { data: Uint8Array; topic: string } | null;
}

export const BreakoutRoomsManager: React.FC<BreakoutRoomsManagerProps> = ({
  isHost = true,
  allParticipants,
  currentParticipantId,
  onJoinSubRoom,
  onReturnToMainRoom,
  onSendData,
  incomingData,
}) => {
  const [subRooms, setSubRooms] = useState<BreakoutSubRoom[]>([
    { id: "room-1", name: "Breakout Room 1", participantIds: [] },
    { id: "room-2", name: "Breakout Room 2", participantIds: [] },
  ]);

  const [isActive, setIsActive] = useState<boolean>(false);
  const [selfSelectAllowed, setSelfSelectAllowed] = useState<boolean>(false);
  const [timerMinutes, setTimerMinutes] = useState<number>(15);
  const [secondsRemaining, setSecondsRemaining] = useState<number>(15 * 60);
  const [isTimerRunning, setIsTimerRunning] = useState<boolean>(false);
  const [broadcastMessage, setBroadcastMessage] = useState<string>("");
  const [latestAnnouncement, setLatestAnnouncement] = useState<string>("");
  const [currentSubRoomId, setCurrentSubRoomId] = useState<string | null>(null);

  // Broadcast breakout state
  const broadcastBreakoutState = useCallback(
    async (overrideState?: Partial<BreakoutState>) => {
      if (!isHost || !onSendData) return;
      try {
        const state: BreakoutState = {
          isActive,
          rooms: subRooms,
          timerSecondsRemaining: secondsRemaining,
          selfSelectAllowed,
          announcement: latestAnnouncement,
          ...overrideState,
        };
        const bytes = new TextEncoder().encode(JSON.stringify(state));
        await onSendData(bytes, "breakout_rooms");
      } catch {}
    },
    [isHost, onSendData, isActive, subRooms, secondsRemaining, selfSelectAllowed, latestAnnouncement]
  );

  // Handle incoming breakout state
  useEffect(() => {
    if (!incomingData || incomingData.topic !== "breakout_rooms") return;
    try {
      const text = new TextDecoder().decode(incomingData.data);
      const state = JSON.parse(text) as BreakoutState;

      setIsActive(state.isActive);
      setSubRooms(state.rooms || []);
      setSecondsRemaining(state.timerSecondsRemaining || 0);
      setSelfSelectAllowed(Boolean(state.selfSelectAllowed));
      if (state.announcement) {
        setLatestAnnouncement(state.announcement);
      }
    } catch {}
  }, [incomingData]);

  // Countdown timer interval
  useEffect(() => {
    if (!isTimerRunning) return;
    const interval = setInterval(() => {
      setSecondsRemaining((prev) => {
        if (prev <= 1) {
          setIsTimerRunning(false);
          return 0;
        }
        return prev - 1;
      });
    }, 1000);
    return () => clearInterval(interval);
  }, [isTimerRunning]);

  const handleAddRoom = () => {
    const newIdx = subRooms.length + 1;
    const newRoom: BreakoutSubRoom = {
      id: `room-${Date.now()}`,
      name: `Breakout Room ${newIdx}`,
      participantIds: [],
    };
    const nextRooms = [...subRooms, newRoom];
    setSubRooms(nextRooms);
    broadcastBreakoutState({ rooms: nextRooms });
  };

  const handleAutoSplit = () => {
    if (subRooms.length === 0) return;
    const updatedRooms = subRooms.map((r) => ({ ...r, participantIds: [] as string[] }));

    // Distribute participants in round-robin fashion
    allParticipants.forEach((p, idx) => {
      const targetRoomIdx = idx % updatedRooms.length;
      updatedRooms[targetRoomIdx].participantIds.push(p.id);
    });

    setSubRooms(updatedRooms);
    broadcastBreakoutState({ rooms: updatedRooms });
  };

  const handleStartBreakout = () => {
    const totalSec = timerMinutes * 60;
    setSecondsRemaining(totalSec);
    setIsActive(true);
    setIsTimerRunning(true);
    broadcastBreakoutState({
      isActive: true,
      timerSecondsRemaining: totalSec,
    });
  };

  const handleEndBreakout = () => {
    setIsActive(false);
    setIsTimerRunning(false);
    setCurrentSubRoomId(null);
    onReturnToMainRoom?.();
    broadcastBreakoutState({ isActive: false, timerSecondsRemaining: 0 });
  };

  const handleSendBroadcast = (e: React.FormEvent) => {
    e.preventDefault();
    if (!broadcastMessage.trim()) return;
    setLatestAnnouncement(broadcastMessage.trim());
    broadcastBreakoutState({ announcement: broadcastMessage.trim() });
    setBroadcastMessage("");
  };

  const handleJoinRoom = (roomId: string) => {
    setCurrentSubRoomId(roomId);
    onJoinSubRoom?.(roomId);
  };

  const handleLeaveRoom = () => {
    setCurrentSubRoomId(null);
    onReturnToMainRoom?.();
  };

  const formatTimer = (totalSec: number) => {
    const m = Math.floor(totalSec / 60);
    const s = totalSec % 60;
    return `${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
  };

  return (
    <div
      data-testid="breakout-rooms-manager"
      className="flex flex-col h-full w-full bg-slate-900 border border-slate-800 rounded-xl overflow-hidden shadow-2xl"
    >
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 bg-slate-950 border-b border-slate-800">
        <div className="flex items-center space-x-2">
          <Split className="w-4 h-4 text-emerald-400" />
          <h2 className="text-sm font-semibold text-white">Breakout Rooms Manager</h2>
        </div>

        {/* Status / Timer */}
        {isActive && (
          <div className="flex items-center space-x-1.5 px-2.5 py-1 bg-emerald-950/40 border border-emerald-500/30 rounded-full text-emerald-400 font-mono text-xs">
            <Clock className="w-3.5 h-3.5" />
            <span>{formatTimer(secondsRemaining)}</span>
          </div>
        )}
      </div>

      {/* Host Announcement Banner */}
      {latestAnnouncement && (
        <div className="px-4 py-2 bg-amber-500/10 border-b border-amber-500/20 text-amber-300 text-xs flex items-center space-x-2 animate-in fade-in">
          <Megaphone className="w-4 h-4 shrink-0 text-amber-400" />
          <span className="font-medium">Host Announcement:</span>
          <span className="truncate">{latestAnnouncement}</span>
        </div>
      )}

      {/* Host Controls Bar */}
      {isHost && (
        <div className="p-3 bg-slate-950/50 border-b border-slate-800 flex items-center justify-between flex-wrap gap-2 text-xs">
          <div className="flex items-center space-x-2">
            {!isActive ? (
              <>
                <button
                  type="button"
                  onClick={handleAddRoom}
                  className="px-2.5 py-1 bg-slate-800 hover:bg-slate-700 text-white rounded-lg border border-slate-700 transition-colors flex items-center space-x-1"
                >
                  <Plus className="w-3.5 h-3.5" />
                  <span>Add Room</span>
                </button>
                <button
                  type="button"
                  onClick={handleAutoSplit}
                  className="px-2.5 py-1 bg-slate-800 hover:bg-slate-700 text-white rounded-lg border border-slate-700 transition-colors flex items-center space-x-1"
                >
                  <Shuffle className="w-3.5 h-3.5" />
                  <span>Auto Distribute</span>
                </button>
              </>
            ) : (
              <button
                type="button"
                onClick={handleEndBreakout}
                className="px-3 py-1 bg-red-600 hover:bg-red-500 text-white rounded-lg transition-colors flex items-center space-x-1 font-medium"
              >
                <Square className="w-3.5 h-3.5" />
                <span>Close All Breakout Rooms</span>
              </button>
            )}
          </div>

          {!isActive && (
            <div className="flex items-center space-x-2">
              <label className="text-slate-400">Duration (min):</label>
              <input
                type="number"
                min={1}
                max={120}
                value={timerMinutes}
                onChange={(e) => setTimerMinutes(Number(e.target.value))}
                className="w-14 bg-slate-800 border border-slate-700 rounded px-2 py-0.5 text-white text-center"
              />
              <button
                type="button"
                onClick={handleStartBreakout}
                className="px-3 py-1 bg-emerald-600 hover:bg-emerald-500 text-white rounded-lg transition-colors flex items-center space-x-1 font-medium"
              >
                <Play className="w-3.5 h-3.5" />
                <span>Start Breakouts</span>
              </button>
            </div>
          )}
        </div>
      )}

      {/* Host Broadcast Message Input */}
      {isHost && isActive && (
        <form onSubmit={handleSendBroadcast} className="px-3 py-2 bg-slate-950/80 border-b border-slate-800 flex space-x-2">
          <input
            type="text"
            placeholder="Broadcast announcement to all sub-rooms..."
            value={broadcastMessage}
            onChange={(e) => setBroadcastMessage(e.target.value)}
            className="flex-1 bg-slate-800 border border-slate-700 rounded-lg px-3 py-1.5 text-xs text-white placeholder-slate-500 focus:outline-none focus:ring-1 focus:ring-emerald-500"
          />
          <button
            type="submit"
            className="px-3 py-1.5 bg-emerald-600 hover:bg-emerald-500 text-white rounded-lg text-xs font-medium transition-colors flex items-center space-x-1"
          >
            <Send className="w-3 h-3" />
            <span>Broadcast</span>
          </button>
        </form>
      )}

      {/* Sub-Rooms List */}
      <div className="flex-1 overflow-y-auto p-4 space-y-3">
        {subRooms.map((room) => {
          const isUserInThisRoom = currentSubRoomId === room.id;
          const assignedNames = room.participantIds
            .map((id) => allParticipants.find((p) => p.id === id)?.name || id)
            .join(", ");

          return (
            <div
              key={room.id}
              className={`p-3.5 rounded-xl border transition-all text-xs flex items-center justify-between ${
                isUserInThisRoom
                  ? "bg-emerald-950/20 border-emerald-500/50 shadow-md"
                  : "bg-slate-800/60 border-slate-700/60"
              }`}
            >
              <div className="min-w-0 pr-3">
                <div className="flex items-center space-x-2">
                  <h3 className="font-semibold text-slate-200">{room.name}</h3>
                  <span className="text-[10px] bg-slate-700 px-1.5 py-0.2 rounded-full text-slate-300">
                    {room.participantIds.length} members
                  </span>
                </div>
                <p className="text-[11px] text-slate-400 mt-1 truncate">
                  {assignedNames || "No participants assigned"}
                </p>
              </div>

              {/* Join / Switch Button */}
              <div>
                {isUserInThisRoom ? (
                  <button
                    type="button"
                    onClick={handleLeaveRoom}
                    className="px-3 py-1.5 rounded-lg bg-red-600/20 text-red-300 hover:bg-red-600 hover:text-white transition-colors flex items-center space-x-1"
                  >
                    <DoorOpen className="w-3.5 h-3.5" />
                    <span>Return to Main</span>
                  </button>
                ) : (
                  (isHost || selfSelectAllowed || room.participantIds.includes(currentParticipantId)) && (
                    <button
                      type="button"
                      onClick={() => handleJoinRoom(room.id)}
                      className="px-3 py-1.5 rounded-lg bg-emerald-600 hover:bg-emerald-500 text-white font-medium transition-colors flex items-center space-x-1"
                    >
                      <ArrowRightLeft className="w-3.5 h-3.5" />
                      <span>Join Room</span>
                    </button>
                  )
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};
