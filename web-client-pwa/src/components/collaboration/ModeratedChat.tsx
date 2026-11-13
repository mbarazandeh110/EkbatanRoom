"use client";

import React, { useState, useEffect, useRef, useCallback } from "react";
import { Send, Shield, Check, X, Hand, ArrowUpRight, MessageSquare } from "lucide-react";

export interface ChatMessage {
  id: string;
  senderId: string;
  senderName: string;
  text: string;
  timestamp: number;
  status: "pending" | "approved" | "rejected";
  isPrivate?: boolean;
  targetId?: string;
}

export interface ModeratedChatProps {
  roomId: string;
  isHost?: boolean;
  currentParticipant: { id: string; name: string };
  raisedHands?: { id: string; name: string }[];
  onPromoteToStage?: (participantId: string) => void;
  onLowerHand?: (participantId: string) => void;
  onSendData?: (data: Uint8Array, topic: string) => Promise<void>;
  incomingData?: { data: Uint8Array; topic: string } | null;
}

export const ModeratedChat: React.FC<ModeratedChatProps> = ({
  roomId,
  isHost = false,
  currentParticipant,
  raisedHands = [],
  onPromoteToStage,
  onLowerHand,
  onSendData,
  incomingData,
}) => {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [inputText, setInputText] = useState<string>("");
  const [moderationEnabled, setModerationEnabled] = useState<boolean>(false);
  const [activeTab, setActiveTab] = useState<"chat" | "moderation" | "hands">("chat");
  const messagesEndRef = useRef<HTMLDivElement>(null);

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  };

  useEffect(() => {
    scrollToBottom();
  }, [messages]);

  // Broadcast chat message / moderation action
  const broadcastChatPayload = useCallback(
    async (payload: unknown) => {
      if (!onSendData) return;
      try {
        const bytes = new TextEncoder().encode(JSON.stringify(payload));
        await onSendData(bytes, "room_chat");
      } catch {}
    },
    [onSendData]
  );

  // Handle incoming chat messages and moderation events
  useEffect(() => {
    if (!incomingData || incomingData.topic !== "room_chat") return;
    try {
      const text = new TextDecoder().decode(incomingData.data);
      const action = JSON.parse(text) as {
        type: "new_message" | "approve_message" | "reject_message" | "toggle_moderation";
        message?: ChatMessage;
        messageId?: string;
        enabled?: boolean;
      };

      if (action.type === "new_message" && action.message) {
        setMessages((prev) => [...prev, action.message!]);
      } else if (action.type === "approve_message" && action.messageId) {
        setMessages((prev) =>
          prev.map((m) => (m.id === action.messageId ? { ...m, status: "approved" } : m))
        );
      } else if (action.type === "reject_message" && action.messageId) {
        setMessages((prev) =>
          prev.map((m) => (m.id === action.messageId ? { ...m, status: "rejected" } : m))
        );
      } else if (action.type === "toggle_moderation" && action.enabled !== undefined) {
        setModerationEnabled(action.enabled);
      }
    } catch {}
  }, [incomingData]);

  const handleSendMessage = (e: React.FormEvent) => {
    e.preventDefault();
    if (!inputText.trim()) return;

    const initialStatus: ChatMessage["status"] = isHost || !moderationEnabled ? "approved" : "pending";

    const msg: ChatMessage = {
      id: `msg-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
      senderId: currentParticipant.id,
      senderName: currentParticipant.name,
      text: inputText.trim(),
      timestamp: Date.now(),
      status: initialStatus,
    };

    setMessages((prev) => [...prev, msg]);
    setInputText("");

    broadcastChatPayload({ type: "new_message", message: msg });
  };

  const handleApprove = (msgId: string) => {
    setMessages((prev) =>
      prev.map((m) => (m.id === msgId ? { ...m, status: "approved" } : m))
    );
    broadcastChatPayload({ type: "approve_message", messageId: msgId });
  };

  const handleReject = (msgId: string) => {
    setMessages((prev) =>
      prev.map((m) => (m.id === msgId ? { ...m, status: "rejected" } : m))
    );
    broadcastChatPayload({ type: "reject_message", messageId: msgId });
  };

  const handleToggleModeration = () => {
    const next = !moderationEnabled;
    setModerationEnabled(next);
    broadcastChatPayload({ type: "toggle_moderation", enabled: next });
  };

  // Filter messages visible to current user
  const visibleMessages = messages.filter((m) => {
    if (m.status === "approved") return true;
    if (isHost) return true; // Host sees pending and rejected
    if (m.senderId === currentParticipant.id) return true; // Sender sees their own pending
    return false;
  });

  const pendingCount = messages.filter((m) => m.status === "pending").length;

  return (
    <div
      data-testid="moderated-chat"
      className="flex flex-col h-full w-full bg-slate-900 border border-slate-800 rounded-xl overflow-hidden shadow-2xl"
    >
      {/* Tab Navigation & Moderation Switch */}
      <div className="flex items-center justify-between px-3 py-2 bg-slate-950 border-b border-slate-800 text-xs">
        <div className="flex space-x-1">
          <button
            type="button"
            onClick={() => setActiveTab("chat")}
            className={`px-2.5 py-1 rounded-lg transition-colors font-medium ${
              activeTab === "chat" ? "bg-slate-800 text-white" : "text-slate-400 hover:text-white"
            }`}
          >
            Chat
          </button>

          {isHost && (
            <button
              type="button"
              onClick={() => setActiveTab("moderation")}
              className={`px-2.5 py-1 rounded-lg transition-colors font-medium flex items-center space-x-1 ${
                activeTab === "moderation" ? "bg-slate-800 text-white" : "text-slate-400 hover:text-white"
              }`}
            >
              <span>Queue</span>
              {pendingCount > 0 && (
                <span className="px-1.5 py-0.2 bg-amber-500/20 text-amber-400 rounded-full text-[10px]">
                  {pendingCount}
                </span>
              )}
            </button>
          )}

          {isHost && (
            <button
              type="button"
              onClick={() => setActiveTab("hands")}
              className={`px-2.5 py-1 rounded-lg transition-colors font-medium flex items-center space-x-1 ${
                activeTab === "hands" ? "bg-slate-800 text-white" : "text-slate-400 hover:text-white"
              }`}
            >
              <Hand className="w-3 h-3 text-amber-400" />
              <span>Stage ({raisedHands.length})</span>
            </button>
          )}
        </div>

        {/* Host Moderation Toggle */}
        {isHost && (
          <button
            type="button"
            onClick={handleToggleModeration}
            aria-label="Toggle chat moderation"
            className={`px-2 py-0.5 rounded text-[10px] font-medium border flex items-center space-x-1 transition-colors ${
              moderationEnabled
                ? "bg-amber-500/10 text-amber-400 border-amber-500/30"
                : "bg-slate-800 text-slate-400 border-slate-700"
            }`}
          >
            <Shield className="w-3 h-3" />
            <span>{moderationEnabled ? "Moderation: ON" : "Moderation: OFF"}</span>
          </button>
        )}
      </div>

      {/* Main Tab Views */}
      {activeTab === "chat" && (
        <div className="flex-1 flex flex-col min-h-0">
          {/* Messages Feed */}
          <div className="flex-1 overflow-y-auto p-3 space-y-3">
            {visibleMessages.length === 0 ? (
              <div className="h-full flex flex-col items-center justify-center text-slate-500 text-xs py-8">
                <MessageSquare className="w-8 h-8 text-slate-700 mb-2" />
                <p>Welcome to #{roomId} chat.</p>
              </div>
            ) : (
              visibleMessages.map((msg) => {
                const isMe = msg.senderId === currentParticipant.id;
                return (
                  <div
                    key={msg.id}
                    className={`flex flex-col ${isMe ? "items-end" : "items-start"}`}
                  >
                    <div className="flex items-center space-x-1 text-[10px] text-slate-400 mb-0.5">
                      <span className="font-semibold text-slate-300">{msg.senderName}</span>
                      <span>•</span>
                      <span>{new Date(msg.timestamp).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</span>
                      {msg.status === "pending" && (
                        <span className="text-amber-400 ml-1">(Pending Approval)</span>
                      )}
                    </div>
                    <div
                      className={`max-w-[85%] rounded-xl px-3 py-2 text-xs break-words ${
                        isMe
                          ? "bg-emerald-600 text-white rounded-br-none"
                          : "bg-slate-800 text-slate-100 rounded-bl-none border border-slate-700/60"
                      }`}
                    >
                      {msg.text}
                    </div>
                  </div>
                );
              })
            )}
            <div ref={messagesEndRef} />
          </div>

          {/* Send Input */}
          <form onSubmit={handleSendMessage} className="p-2.5 bg-slate-950 border-t border-slate-800 flex space-x-2">
            <input
              type="text"
              placeholder={moderationEnabled && !isHost ? "Message (pending moderation)..." : "Type a message..."}
              value={inputText}
              onChange={(e) => setInputText(e.target.value)}
              className="flex-1 bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-xs text-white placeholder-slate-500 focus:outline-none focus:ring-1 focus:ring-emerald-500"
            />
            <button
              type="submit"
              disabled={!inputText.trim()}
              aria-label="Send message"
              className="p-2 bg-emerald-600 hover:bg-emerald-500 disabled:opacity-40 text-white rounded-lg transition-colors shrink-0"
            >
              <Send className="w-4 h-4" />
            </button>
          </form>
        </div>
      )}

      {/* Moderation Queue Tab */}
      {activeTab === "moderation" && isHost && (
        <div className="flex-1 overflow-y-auto p-3 space-y-2">
          {messages.filter((m) => m.status === "pending").length === 0 ? (
            <div className="h-full flex flex-col items-center justify-center text-slate-500 text-xs py-8">
              <Check className="w-8 h-8 text-emerald-500/40 mb-2" />
              <p>No messages pending operator approval.</p>
            </div>
          ) : (
            messages
              .filter((m) => m.status === "pending")
              .map((msg) => (
                <div
                  key={msg.id}
                  className="p-3 rounded-lg bg-slate-800/80 border border-slate-700 flex items-center justify-between text-xs"
                >
                  <div className="min-w-0 pr-2">
                    <div className="font-semibold text-slate-200">{msg.senderName}</div>
                    <p className="text-slate-300 mt-0.5 break-words">{msg.text}</p>
                  </div>
                  <div className="flex items-center space-x-1 shrink-0">
                    <button
                      type="button"
                      onClick={() => handleApprove(msg.id)}
                      aria-label="Approve message"
                      className="p-1.5 rounded bg-emerald-600/20 text-emerald-400 hover:bg-emerald-600 hover:text-white transition-colors"
                    >
                      <Check className="w-3.5 h-3.5" />
                    </button>
                    <button
                      type="button"
                      onClick={() => handleReject(msg.id)}
                      aria-label="Reject message"
                      className="p-1.5 rounded bg-red-600/20 text-red-400 hover:bg-red-600 hover:text-white transition-colors"
                    >
                      <X className="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>
              ))
          )}
        </div>
      )}

      {/* Raised Hands / Stage Promotion Tab */}
      {activeTab === "hands" && isHost && (
        <div className="flex-1 overflow-y-auto p-3 space-y-2">
          {raisedHands.length === 0 ? (
            <div className="h-full flex flex-col items-center justify-center text-slate-500 text-xs py-8">
              <Hand className="w-8 h-8 text-slate-700 mb-2" />
              <p>No raised hands in queue.</p>
            </div>
          ) : (
            raisedHands.map((participant) => (
              <div
                key={participant.id}
                className="p-3 rounded-lg bg-slate-800/80 border border-slate-700 flex items-center justify-between text-xs"
              >
                <div className="flex items-center space-x-2">
                  <Hand className="w-4 h-4 text-amber-400 animate-bounce" />
                  <span className="font-medium text-slate-200">{participant.name}</span>
                </div>
                <div className="flex items-center space-x-1.5">
                  <button
                    type="button"
                    onClick={() => onPromoteToStage?.(participant.id)}
                    className="px-2.5 py-1 rounded bg-emerald-600 hover:bg-emerald-500 text-white font-medium text-[11px] flex items-center space-x-1"
                  >
                    <ArrowUpRight className="w-3 h-3" />
                    <span>Promote to Stage</span>
                  </button>
                  <button
                    type="button"
                    onClick={() => onLowerHand?.(participant.id)}
                    className="p-1 rounded text-slate-400 hover:text-red-400 hover:bg-slate-700"
                  >
                    <X className="w-3.5 h-3.5" />
                  </button>
                </div>
              </div>
            ))
          )}
        </div>
      )}
    </div>
  );
};
