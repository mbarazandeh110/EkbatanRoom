"use client";

import React, { useState, useEffect, useCallback } from "react";
import { BarChart3, Plus, CheckCircle2, Award } from "lucide-react";

export interface PollOption {
  id: string;
  text: string;
  votes: number;
}

export interface PollModel {
  id: string;
  question: string;
  options: PollOption[];
  isClosed: boolean;
  isQuiz: boolean;
  correctOptionId?: string;
  createdBy: string;
  createdAt: number;
  totalVotes: number;
}

export interface PollActionPayload {
  type: "create" | "vote" | "close";
  pollId: string;
  poll?: PollModel;
  optionId?: string;
}

export interface LivePollsProps {
  isHost?: boolean;
  currentParticipantId: string;
  onSendData?: (data: Uint8Array, topic: string) => Promise<void>;
  incomingData?: { data: Uint8Array; topic: string } | null;
}

export const LivePolls: React.FC<LivePollsProps> = ({
  isHost = true,
  currentParticipantId,
  onSendData,
  incomingData,
}) => {
  const [polls, setPolls] = useState<PollModel[]>([]);
  const [userVotes, setUserVotes] = useState<Record<string, string>>({}); // pollId -> optionId
  const [isCreating, setIsCreating] = useState<boolean>(false);

  // Poll creation form state
  const [newQuestion, setNewQuestion] = useState<string>("");
  const [newOptions, setNewOptions] = useState<string[]>(["Option 1", "Option 2"]);
  const [isQuiz, setIsQuiz] = useState<boolean>(false);
  const [correctOptionIdx, setCorrectOptionIdx] = useState<number>(0);

  // Broadcast poll actions
  const broadcastAction = useCallback(
    async (payload: PollActionPayload) => {
      if (!onSendData) return;
      try {
        const bytes = new TextEncoder().encode(JSON.stringify(payload));
        await onSendData(bytes, "room_polls");
      } catch {}
    },
    [onSendData]
  );

  // Handle incoming DataChannel messages
  useEffect(() => {
    if (!incomingData || incomingData.topic !== "room_polls") return;
    try {
      const text = new TextDecoder().decode(incomingData.data);
      const action = JSON.parse(text) as PollActionPayload;

      if (action.type === "create" && action.poll) {
        setPolls((prev) => [action.poll!, ...prev.filter((p) => p.id !== action.poll!.id)]);
      } else if (action.type === "vote" && action.pollId && action.optionId) {
        setPolls((prev) =>
          prev.map((poll) => {
            if (poll.id !== action.pollId) return poll;
            return {
              ...poll,
              totalVotes: poll.totalVotes + 1,
              options: poll.options.map((opt) =>
                opt.id === action.optionId ? { ...opt, votes: opt.votes + 1 } : opt
              ),
            };
          })
        );
      } else if (action.type === "close" && action.pollId) {
        setPolls((prev) =>
          prev.map((poll) =>
            poll.id === action.pollId ? { ...poll, isClosed: true } : poll
          )
        );
      }
    } catch {}
  }, [incomingData]);

  const handleCreatePoll = (e: React.FormEvent) => {
    e.preventDefault();
    if (!newQuestion.trim()) return;

    const pollId = `poll-${Date.now()}`;
    const generatedOptions: PollOption[] = newOptions
      .filter((opt) => opt.trim())
      .map((opt, idx) => ({
        id: `opt-${idx}-${Math.random().toString(36).slice(2, 6)}`,
        text: opt.trim(),
        votes: 0,
      }));

    if (generatedOptions.length < 2) return;

    const newPoll: PollModel = {
      id: pollId,
      question: newQuestion.trim(),
      options: generatedOptions,
      isClosed: false,
      isQuiz,
      correctOptionId: isQuiz ? generatedOptions[correctOptionIdx]?.id : undefined,
      createdBy: currentParticipantId,
      createdAt: Date.now(),
      totalVotes: 0,
    };

    setPolls((prev) => [newPoll, ...prev]);
    setIsCreating(false);
    setNewQuestion("");
    setNewOptions(["Option 1", "Option 2"]);

    broadcastAction({ type: "create", pollId: newPoll.id, poll: newPoll });
  };

  const handleVote = (pollId: string, optionId: string) => {
    if (userVotes[pollId]) return; // Already voted

    setUserVotes((prev) => ({ ...prev, [pollId]: optionId }));

    setPolls((prev) =>
      prev.map((poll) => {
        if (poll.id !== pollId) return poll;
        return {
          ...poll,
          totalVotes: poll.totalVotes + 1,
          options: poll.options.map((opt) =>
            opt.id === optionId ? { ...opt, votes: opt.votes + 1 } : opt
          ),
        };
      })
    );

    broadcastAction({ type: "vote", pollId, optionId });
  };

  const handleClosePoll = (pollId: string) => {
    if (!isHost) return;
    setPolls((prev) =>
      prev.map((p) => (p.id === pollId ? { ...p, isClosed: true } : p))
    );
    broadcastAction({ type: "close", pollId });
  };

  return (
    <div
      data-testid="live-polls"
      className="flex flex-col h-full w-full bg-slate-900 border border-slate-800 rounded-xl overflow-hidden shadow-2xl"
    >
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 bg-slate-950 border-b border-slate-800">
        <div className="flex items-center space-x-2">
          <BarChart3 className="w-4 h-4 text-emerald-400" />
          <h2 className="text-sm font-semibold text-white">Live Polls & Quizzes</h2>
        </div>
        {isHost && (
          <button
            type="button"
            onClick={() => setIsCreating(!isCreating)}
            aria-label={isCreating ? "Cancel poll creation" : "Create new poll"}
            className="px-3 py-1 bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-medium rounded-lg transition-colors flex items-center space-x-1"
          >
            <Plus className="w-3.5 h-3.5" />
            <span>{isCreating ? "Cancel" : "New Poll"}</span>
          </button>
        )}
      </div>

      {/* Poll Creation Form */}
      {isCreating && (
        <form onSubmit={handleCreatePoll} className="p-4 bg-slate-950/70 border-b border-slate-800 space-y-3">
          <div>
            <label className="block text-xs font-medium text-slate-300 mb-1">Question / Prompt</label>
            <input
              type="text"
              required
              placeholder="e.g. Which WebRTC codec is optimal for mobile?"
              value={newQuestion}
              onChange={(e) => setNewQuestion(e.target.value)}
              className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-1.5 text-xs text-white placeholder-slate-500 focus:outline-none focus:ring-1 focus:ring-emerald-500"
            />
          </div>

          <div className="space-y-2">
            <label className="block text-xs font-medium text-slate-300">Answer Options</label>
            {newOptions.map((opt, idx) => (
              <div key={idx} className="flex items-center space-x-2">
                <input
                  type="text"
                  required
                  value={opt}
                  onChange={(e) => {
                    const copy = [...newOptions];
                    copy[idx] = e.target.value;
                    setNewOptions(copy);
                  }}
                  className="flex-1 bg-slate-800 border border-slate-700 rounded-lg px-3 py-1 text-xs text-white focus:outline-none focus:ring-1 focus:ring-emerald-500"
                />
                {isQuiz && (
                  <button
                    type="button"
                    onClick={() => setCorrectOptionIdx(idx)}
                    aria-label={`Mark option ${idx + 1} as correct`}
                    className={`px-2 py-1 text-[10px] rounded border ${
                      correctOptionIdx === idx
                        ? "bg-emerald-600/30 text-emerald-400 border-emerald-500"
                        : "text-slate-500 border-slate-700"
                    }`}
                  >
                    Correct
                  </button>
                )}
              </div>
            ))}

            {newOptions.length < 5 && (
              <button
                type="button"
                onClick={() => setNewOptions([...newOptions, `Option ${newOptions.length + 1}`])}
                className="text-xs text-emerald-400 hover:text-emerald-300 transition-colors flex items-center space-x-1 mt-1"
              >
                <Plus className="w-3 h-3" />
                <span>Add Option</span>
              </button>
            )}
          </div>

          <div className="flex items-center justify-between pt-2">
            <label className="flex items-center space-x-2 text-xs text-slate-300 cursor-pointer">
              <input
                type="checkbox"
                checked={isQuiz}
                onChange={(e) => setIsQuiz(e.target.checked)}
                className="rounded bg-slate-800 border-slate-700 text-emerald-500 focus:ring-emerald-500"
              />
              <span>Quiz Mode (Highlight correct answer upon close)</span>
            </label>

            <button
              type="submit"
              className="px-4 py-1.5 bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-medium rounded-lg shadow-md transition-colors"
            >
              Launch Poll
            </button>
          </div>
        </form>
      )}

      {/* Polls Feed */}
      <div className="flex-1 overflow-y-auto p-4 space-y-4">
        {polls.length === 0 ? (
          <div className="h-full flex flex-col items-center justify-center text-slate-500 text-xs py-8">
            <BarChart3 className="w-10 h-10 text-slate-700 mb-2" />
            <p>No active polls yet in this session.</p>
          </div>
        ) : (
          polls.map((poll) => {
            const hasVoted = Boolean(userVotes[poll.id]);
            const selectedOptId = userVotes[poll.id];

            return (
              <div
                key={poll.id}
                className="p-4 rounded-xl bg-slate-800/60 border border-slate-700/60 space-y-3"
              >
                <div className="flex items-start justify-between">
                  <div className="flex items-center space-x-2">
                    {poll.isQuiz && <Award className="w-4 h-4 text-amber-400 shrink-0" />}
                    <h3 className="text-sm font-semibold text-white">{poll.question}</h3>
                  </div>
                  {poll.isClosed ? (
                    <span className="px-2 py-0.5 rounded-full text-[10px] bg-slate-700 text-slate-300 font-medium">
                      Closed
                    </span>
                  ) : (
                    isHost && (
                      <button
                        type="button"
                        onClick={() => handleClosePoll(poll.id)}
                        className="text-[11px] text-red-400 hover:text-red-300 transition-colors"
                      >
                        End Poll
                      </button>
                    )
                  )}
                </div>

                {/* Options List with Percentage Bars */}
                <div className="space-y-2">
                  {poll.options.map((opt) => {
                    const percentage =
                      poll.totalVotes > 0
                        ? Math.round((opt.votes / poll.totalVotes) * 100)
                        : 0;

                    const isChosen = selectedOptId === opt.id;
                    const isCorrect = poll.isClosed && poll.isQuiz && poll.correctOptionId === opt.id;

                    return (
                      <button
                        key={opt.id}
                        type="button"
                        disabled={hasVoted || poll.isClosed}
                        onClick={() => handleVote(poll.id, opt.id)}
                        className={`w-full relative overflow-hidden text-left p-2.5 rounded-lg border transition-all text-xs flex items-center justify-between ${
                          isChosen
                            ? "border-emerald-500 bg-emerald-950/30 text-white"
                            : isCorrect
                            ? "border-emerald-500 bg-emerald-900/30 text-emerald-200"
                            : "border-slate-700/80 bg-slate-800/40 text-slate-200 hover:border-slate-600"
                        }`}
                      >
                        {/* Fill Progress Bar */}
                        <div
                          className={`absolute top-0 bottom-0 left-0 transition-all duration-300 ${
                            isCorrect
                              ? "bg-emerald-500/30"
                              : isChosen
                              ? "bg-emerald-600/30"
                              : "bg-slate-700/40"
                          }`}
                          style={{ width: `${percentage}%` }}
                        />

                        <div className="relative z-10 flex items-center space-x-2">
                          {isChosen && <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />}
                          <span>{opt.text}</span>
                        </div>

                        <div className="relative z-10 font-mono text-[11px] text-slate-400">
                          {percentage}% ({opt.votes})
                        </div>
                      </button>
                    );
                  })}
                </div>

                <div className="text-[10px] text-slate-500 flex justify-between">
                  <span>Total votes: {poll.totalVotes}</span>
                  {hasVoted && <span className="text-emerald-400 font-medium">Vote Recorded</span>}
                </div>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
};
