"use client";

import React, { useState, useEffect, useCallback } from "react";
import { HelpCircle, ThumbsUp, CheckCircle, Plus } from "lucide-react";

export interface QuestionItem {
  id: string;
  askerId: string;
  askerName: string;
  text: string;
  upvotes: number;
  upvotedBy: string[]; // participantIds
  isAnswered: boolean;
  createdAt: number;
}

export interface QnAActionPayload {
  type: "ask" | "upvote" | "mark_answered";
  questionId?: string;
  question?: QuestionItem;
  participantId?: string;
}

export interface QnAPanelProps {
  isHost?: boolean;
  currentParticipant: { id: string; name: string };
  onSendData?: (data: Uint8Array, topic: string) => Promise<void>;
  incomingData?: { data: Uint8Array; topic: string } | null;
}

export const QnAPanel: React.FC<QnAPanelProps> = ({
  isHost = false,
  currentParticipant,
  onSendData,
  incomingData,
}) => {
  const [questions, setQuestions] = useState<QuestionItem[]>([]);
  const [inputText, setInputText] = useState<string>("");
  const [filterMode, setFilterMode] = useState<"top" | "recent">("top");
  const [hideAnswered, setHideAnswered] = useState<boolean>(false);

  // Broadcast helper
  const broadcastAction = useCallback(
    async (payload: QnAActionPayload) => {
      if (!onSendData) return;
      try {
        const bytes = new TextEncoder().encode(JSON.stringify(payload));
        await onSendData(bytes, "room_qna");
      } catch {}
    },
    [onSendData]
  );

  // Process incoming Q&A updates
  useEffect(() => {
    if (!incomingData || incomingData.topic !== "room_qna") return;
    try {
      const text = new TextDecoder().decode(incomingData.data);
      const action = JSON.parse(text) as QnAActionPayload;

      if (action.type === "ask" && action.question) {
        setQuestions((prev) => [action.question!, ...prev.filter((q) => q.id !== action.question!.id)]);
      } else if (action.type === "upvote" && action.questionId && action.participantId) {
        setQuestions((prev) =>
          prev.map((q) => {
            if (q.id !== action.questionId) return q;
            const hasVoted = q.upvotedBy.includes(action.participantId!);
            const nextUpvotedBy = hasVoted
              ? q.upvotedBy.filter((id) => id !== action.participantId)
              : [...q.upvotedBy, action.participantId!];
            return {
              ...q,
              upvotedBy: nextUpvotedBy,
              upvotes: nextUpvotedBy.length,
            };
          })
        );
      } else if (action.type === "mark_answered" && action.questionId) {
        setQuestions((prev) =>
          prev.map((q) => (q.id === action.questionId ? { ...q, isAnswered: true } : q))
        );
      }
    } catch {}
  }, [incomingData]);

  const handleAskQuestion = (e: React.FormEvent) => {
    e.preventDefault();
    if (!inputText.trim()) return;

    const newQuestion: QuestionItem = {
      id: `q-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
      askerId: currentParticipant.id,
      askerName: currentParticipant.name,
      text: inputText.trim(),
      upvotes: 0,
      upvotedBy: [],
      isAnswered: false,
      createdAt: Date.now(),
    };

    setQuestions((prev) => [newQuestion, ...prev]);
    setInputText("");

    broadcastAction({ type: "ask", question: newQuestion });
  };

  const handleToggleUpvote = (questionId: string) => {
    const q = questions.find((item) => item.id === questionId);
    if (!q) return;

    const hasVoted = q.upvotedBy.includes(currentParticipant.id);
    const nextUpvotedBy = hasVoted
      ? q.upvotedBy.filter((id) => id !== currentParticipant.id)
      : [...q.upvotedBy, currentParticipant.id];

    setQuestions((prev) =>
      prev.map((item) =>
        item.id === questionId
          ? { ...item, upvotedBy: nextUpvotedBy, upvotes: nextUpvotedBy.length }
          : item
      )
    );

    broadcastAction({
      type: "upvote",
      questionId,
      participantId: currentParticipant.id,
    });
  };

  const handleMarkAnswered = (questionId: string) => {
    if (!isHost) return;
    setQuestions((prev) =>
      prev.map((item) =>
        item.id === questionId ? { ...item, isAnswered: true } : item
      )
    );
    broadcastAction({ type: "mark_answered", questionId });
  };

  const displayedQuestions = questions
    .filter((q) => (hideAnswered ? !q.isAnswered : true))
    .sort((a, b) => {
      if (filterMode === "top") {
        return b.upvotes - a.upvotes || b.createdAt - a.createdAt;
      }
      return b.createdAt - a.createdAt;
    });

  return (
    <div
      data-testid="qna-panel"
      className="flex flex-col h-full w-full bg-slate-900 border border-slate-800 rounded-xl overflow-hidden shadow-2xl"
    >
      {/* Header & Filter Controls */}
      <div className="flex items-center justify-between px-4 py-3 bg-slate-950 border-b border-slate-800">
        <div className="flex items-center space-x-2">
          <HelpCircle className="w-4 h-4 text-emerald-400" />
          <h2 className="text-sm font-semibold text-white">Q&A and Upvoting</h2>
        </div>

        <div className="flex items-center space-x-2 text-xs">
          <button
            type="button"
            onClick={() => setFilterMode("top")}
            className={`px-2 py-0.5 rounded font-medium ${
              filterMode === "top" ? "bg-slate-800 text-white" : "text-slate-400 hover:text-white"
            }`}
          >
            Top
          </button>
          <button
            type="button"
            onClick={() => setFilterMode("recent")}
            className={`px-2 py-0.5 rounded font-medium ${
              filterMode === "recent" ? "bg-slate-800 text-white" : "text-slate-400 hover:text-white"
            }`}
          >
            Recent
          </button>
          <label className="flex items-center space-x-1 text-[11px] text-slate-400 cursor-pointer ml-1">
            <input
              type="checkbox"
              checked={hideAnswered}
              onChange={(e) => setHideAnswered(e.target.checked)}
              className="rounded bg-slate-800 border-slate-700 text-emerald-500"
            />
            <span>Hide Answered</span>
          </label>
        </div>
      </div>

      {/* Questions Feed */}
      <div className="flex-1 overflow-y-auto p-4 space-y-3">
        {displayedQuestions.length === 0 ? (
          <div className="h-full flex flex-col items-center justify-center text-slate-500 text-xs py-8">
            <HelpCircle className="w-10 h-10 text-slate-700 mb-2" />
            <p>No questions submitted yet. Be the first to ask!</p>
          </div>
        ) : (
          displayedQuestions.map((q) => {
            const hasUpvoted = q.upvotedBy.includes(currentParticipant.id);

            return (
              <div
                key={q.id}
                className={`p-3 rounded-xl border transition-colors flex items-start space-x-3 text-xs ${
                  q.isAnswered
                    ? "bg-slate-950/40 border-slate-800/80 opacity-70"
                    : "bg-slate-800/60 border-slate-700/60 hover:bg-slate-800"
                }`}
              >
                {/* Upvote Button with count */}
                <button
                  type="button"
                  onClick={() => handleToggleUpvote(q.id)}
                  aria-label={hasUpvoted ? "Remove upvote" : "Upvote question"}
                  className={`flex flex-col items-center justify-center px-2 py-1.5 rounded-lg border transition-colors shrink-0 ${
                    hasUpvoted
                      ? "bg-emerald-600/20 text-emerald-400 border-emerald-500/40"
                      : "bg-slate-800 text-slate-400 hover:text-white border-slate-700"
                  }`}
                >
                  <ThumbsUp className="w-3.5 h-3.5" />
                  <span className="font-mono text-[11px] font-semibold mt-0.5">{q.upvotes}</span>
                </button>

                <div className="flex-1 min-w-0">
                  <div className="flex items-center justify-between mb-1">
                    <span className="font-semibold text-slate-200">{q.askerName}</span>
                    <span className="text-[10px] text-slate-500">
                      {new Date(q.createdAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
                    </span>
                  </div>

                  <p className="text-slate-100 break-words leading-relaxed">{q.text}</p>

                  <div className="flex items-center space-x-2 mt-2">
                    {q.isAnswered ? (
                      <span className="flex items-center space-x-1 text-[10px] text-emerald-400 font-medium">
                        <CheckCircle className="w-3 h-3" />
                        <span>Answered</span>
                      </span>
                    ) : (
                      isHost && (
                        <button
                          type="button"
                          onClick={() => handleMarkAnswered(q.id)}
                          className="text-[10px] text-emerald-400 hover:underline font-medium"
                        >
                          Mark as Answered
                        </button>
                      )
                    )}
                  </div>
                </div>
              </div>
            );
          })
        )}
      </div>

      {/* Ask Question Input */}
      <form onSubmit={handleAskQuestion} className="p-3 bg-slate-950 border-t border-slate-800 flex space-x-2">
        <input
          type="text"
          placeholder="Ask a question for speaker/instructor..."
          value={inputText}
          onChange={(e) => setInputText(e.target.value)}
          className="flex-1 bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-xs text-white placeholder-slate-500 focus:outline-none focus:ring-1 focus:ring-emerald-500"
        />
        <button
          type="submit"
          disabled={!inputText.trim()}
          aria-label="Submit question"
          className="px-3 py-2 bg-emerald-600 hover:bg-emerald-500 disabled:opacity-40 text-white text-xs font-medium rounded-lg transition-colors flex items-center space-x-1 shrink-0"
        >
          <Plus className="w-3.5 h-3.5" />
          <span>Ask</span>
        </button>
      </form>
    </div>
  );
};
