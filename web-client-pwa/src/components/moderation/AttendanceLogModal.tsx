"use client";

import React, { useState } from "react";
import { Users, X, Search, FileSpreadsheet, FileJson } from "lucide-react";

export interface AttendanceRecord {
  participantId: string;
  name: string;
  identity: string;
  role: string;
  joinedAt: number;
  leftAt?: number;
  durationSeconds: number;
  connectionQuality: string;
}

export interface AttendanceLogModalProps {
  isOpen: boolean;
  onClose: () => void;
  roomName: string;
  records: AttendanceRecord[];
}

export const AttendanceLogModal: React.FC<AttendanceLogModalProps> = ({
  isOpen,
  onClose,
  roomName,
  records,
}) => {
  const [searchQuery, setSearchQuery] = useState<string>("");

  if (!isOpen) return null;

  const filteredRecords = records.filter(
    (r) =>
      r.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      r.identity.toLowerCase().includes(searchQuery.toLowerCase())
  );

  const formatDuration = (sec: number): string => {
    const hours = Math.floor(sec / 3600);
    const minutes = Math.floor((sec % 3600) / 60);
    const seconds = sec % 60;
    if (hours > 0) {
      return `${hours}h ${minutes}m ${seconds}s`;
    }
    return `${minutes}m ${seconds}s`;
  };

  const handleExportCsv = () => {
    const headers = "Participant ID,Name,Identity,Role,Joined At,Left At,Duration (Seconds),Duration (Formatted),Quality\n";
    const rows = records
      .map((r) => {
        const joinedStr = new Date(r.joinedAt).toISOString();
        const leftStr = r.leftAt ? new Date(r.leftAt).toISOString() : "Active";
        const formatted = formatDuration(r.durationSeconds);
        return `"${r.participantId}","${r.name}","${r.identity}","${r.role}","${joinedStr}","${leftStr}",${r.durationSeconds},"${formatted}","${r.connectionQuality}"`;
      })
      .join("\n");

    const blob = new Blob([headers + rows], { type: "text/csv;charset=utf-8;" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `attendance-${roomName}-${new Date().toISOString().slice(0, 10)}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const handleExportJson = () => {
    const jsonStr = JSON.stringify({ roomName, exportedAt: new Date().toISOString(), records }, null, 2);
    const blob = new Blob([jsonStr], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `attendance-${roomName}-${new Date().toISOString().slice(0, 10)}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="attendance-modal-title"
      data-testid="attendance-log-modal"
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-slate-950/80 backdrop-blur-sm animate-in fade-in"
    >
      <div className="w-full max-w-3xl bg-slate-900 border border-slate-800 rounded-2xl shadow-2xl overflow-hidden flex flex-col max-h-[85vh]">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-4 bg-slate-950 border-b border-slate-800">
          <div className="flex items-center space-x-2">
            <Users className="w-4 h-4 text-emerald-400" />
            <div>
              <h2 id="attendance-modal-title" className="text-sm font-semibold text-white">
                Live Attendance & Session Audit Log
              </h2>
              <p className="text-xs text-slate-400">Meeting: {roomName}</p>
            </div>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close attendance modal"
            className="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Toolbar: Search and Export Buttons */}
        <div className="p-4 bg-slate-950/50 border-b border-slate-800 flex items-center justify-between flex-wrap gap-2 text-xs">
          <div className="relative w-64">
            <Search className="absolute left-2.5 top-2.5 w-3.5 h-3.5 text-slate-500" />
            <input
              type="text"
              placeholder="Search participants..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="w-full pl-8 pr-3 py-1.5 bg-slate-800 border border-slate-700 rounded-lg text-white placeholder-slate-500 focus:outline-none focus:ring-1 focus:ring-emerald-500"
            />
          </div>

          <div className="flex items-center space-x-2">
            <button
              type="button"
              onClick={handleExportCsv}
              className="px-3 py-1.5 bg-slate-800 hover:bg-slate-700 text-white rounded-lg border border-slate-700 transition-colors flex items-center space-x-1.5"
            >
              <FileSpreadsheet className="w-3.5 h-3.5 text-emerald-400" />
              <span>Export CSV</span>
            </button>
            <button
              type="button"
              onClick={handleExportJson}
              className="px-3 py-1.5 bg-slate-800 hover:bg-slate-700 text-white rounded-lg border border-slate-700 transition-colors flex items-center space-x-1.5"
            >
              <FileJson className="w-3.5 h-3.5 text-blue-400" />
              <span>Export JSON</span>
            </button>
          </div>
        </div>

        {/* Table Content */}
        <div className="flex-1 overflow-y-auto">
          <table className="w-full text-left text-xs border-collapse">
            <thead>
              <tr className="bg-slate-950/70 border-b border-slate-800 text-slate-400 font-medium">
                <th className="py-2.5 px-4">Participant</th>
                <th className="py-2.5 px-3">Role</th>
                <th className="py-2.5 px-3">Joined</th>
                <th className="py-2.5 px-3">Status</th>
                <th className="py-2.5 px-3">Total Duration</th>
                <th className="py-2.5 px-4">Quality</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-800/60">
              {filteredRecords.length === 0 ? (
                <tr>
                  <td colSpan={6} className="py-8 text-center text-slate-500">
                    No participant records match search.
                  </td>
                </tr>
              ) : (
                filteredRecords.map((r) => (
                  <tr key={r.participantId} className="hover:bg-slate-800/30 transition-colors">
                    <td className="py-2.5 px-4">
                      <div className="font-medium text-slate-200">{r.name}</div>
                      <div className="text-[10px] text-slate-500 font-mono">{r.identity}</div>
                    </td>
                    <td className="py-2.5 px-3 capitalize text-slate-300">{r.role}</td>
                    <td className="py-2.5 px-3 text-slate-400 font-mono text-[11px]">
                      {new Date(r.joinedAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}
                    </td>
                    <td className="py-2.5 px-3">
                      {r.leftAt ? (
                        <span className="text-[10px] text-slate-400 bg-slate-800 px-2 py-0.5 rounded-full">
                          Left
                        </span>
                      ) : (
                        <span className="text-[10px] text-emerald-400 bg-emerald-500/10 px-2 py-0.5 rounded-full font-medium">
                          Active Now
                        </span>
                      )}
                    </td>
                    <td className="py-2.5 px-3 font-mono text-slate-300">
                      {formatDuration(r.durationSeconds)}
                    </td>
                    <td className="py-2.5 px-4 capitalize text-slate-400">
                      {r.connectionQuality}
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
};
