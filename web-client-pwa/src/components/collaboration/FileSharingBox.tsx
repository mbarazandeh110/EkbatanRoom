"use client";

import React, { useState, useEffect, useRef } from "react";
import { UploadCloud, FileText, Download } from "lucide-react";

export interface SharedFileItem {
  id: string;
  name: string;
  size: number;
  uploadedBy: string;
  uploaderId: string;
  timestamp: number;
  downloadUrl: string;
}

export interface FileSharingBoxProps {
  roomId: string;
  currentParticipant: { id: string; name: string };
  onSendData?: (data: Uint8Array, topic: string) => Promise<void>;
  incomingData?: { data: Uint8Array; topic: string } | null;
}

export const FileSharingBox: React.FC<FileSharingBoxProps> = ({
  roomId,
  currentParticipant,
  onSendData,
  incomingData,
}) => {
  const [files, setFiles] = useState<SharedFileItem[]>([]);
  const [isUploading, setIsUploading] = useState<boolean>(false);
  const [uploadProgress, setUploadProgress] = useState<number>(0);
  const fileInputRef = useRef<HTMLInputElement>(null);

  // Receive shared file announcements from DataChannel
  useEffect(() => {
    if (!incomingData || incomingData.topic !== "file_shared") return;
    try {
      const text = new TextDecoder().decode(incomingData.data);
      const newFile = JSON.parse(text) as SharedFileItem;
      setFiles((prev) => {
        if (prev.some((f) => f.id === newFile.id)) return prev;
        return [newFile, ...prev];
      });
    } catch {}
  }, [incomingData]);

  const formatFileSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  const handleFileSelect = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const selectedFile = e.target.files?.[0];
    if (!selectedFile) return;

    setIsUploading(true);
    setUploadProgress(20);

    // Simulate progress / S3 presigned upload
    await new Promise((r) => setTimeout(r, 200));
    setUploadProgress(70);
    await new Promise((r) => setTimeout(r, 200));
    setUploadProgress(100);

    const newFileItem: SharedFileItem = {
      id: `file-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
      name: selectedFile.name,
      size: selectedFile.size,
      uploadedBy: currentParticipant.name,
      uploaderId: currentParticipant.id,
      timestamp: Date.now(),
      downloadUrl: URL.createObjectURL(selectedFile),
    };

    setFiles((prev) => [newFileItem, ...prev]);
    setIsUploading(false);
    setUploadProgress(0);

    // Reset input
    if (fileInputRef.current) {
      fileInputRef.current.value = "";
    }

    // Broadcast file to room peers
    if (onSendData) {
      try {
        const payload = new TextEncoder().encode(JSON.stringify(newFileItem));
        await onSendData(payload, "file_shared");
      } catch {}
    }
  };

  return (
    <div
      data-testid="file-sharing-box"
      className="flex flex-col h-full w-full bg-slate-900 border border-slate-800 rounded-xl overflow-hidden shadow-2xl"
    >
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 bg-slate-950 border-b border-slate-800">
        <div>
          <h2 className="text-sm font-semibold text-white">Room Files & Handouts</h2>
          <p className="text-[11px] text-slate-400">Share documents in {roomId}</p>
        </div>
        <button
          type="button"
          onClick={() => fileInputRef.current?.click()}
          disabled={isUploading}
          aria-label="Upload document"
          className="px-3 py-1.5 bg-emerald-600 hover:bg-emerald-500 disabled:opacity-50 text-white text-xs font-medium rounded-lg transition-colors flex items-center space-x-1.5"
        >
          <UploadCloud className="w-3.5 h-3.5" />
          <span>Upload File</span>
        </button>
        <input
          ref={fileInputRef}
          type="file"
          className="hidden"
          onChange={handleFileSelect}
        />
      </div>

      {/* Upload Progress Bar */}
      {isUploading && (
        <div className="w-full bg-slate-800 h-1">
          <div
            className="bg-emerald-500 h-1 transition-all duration-200"
            style={{ width: `${uploadProgress}%` }}
          />
        </div>
      )}

      {/* File List */}
      <div className="flex-1 overflow-y-auto p-3 space-y-2">
        {files.length === 0 ? (
          <div className="h-full flex flex-col items-center justify-center text-slate-500 text-xs py-8">
            <UploadCloud className="w-10 h-10 text-slate-700 mb-2" />
            <p>No handouts shared yet in this room.</p>
          </div>
        ) : (
          files.map((file) => (
            <div
              key={file.id}
              className="flex items-center justify-between p-3 rounded-lg bg-slate-800/60 border border-slate-700/60 hover:bg-slate-800 transition-colors"
            >
              <div className="flex items-center space-x-3 min-w-0">
                <div className="p-2 rounded-lg bg-emerald-500/10 text-emerald-400 shrink-0">
                  <FileText className="w-5 h-5" />
                </div>
                <div className="min-w-0">
                  <p className="text-xs font-medium text-slate-200 truncate">{file.name}</p>
                  <p className="text-[10px] text-slate-400">
                    {formatFileSize(file.size)} • {file.uploadedBy}
                  </p>
                </div>
              </div>

              <a
                href={file.downloadUrl}
                download={file.name}
                aria-label={`Download ${file.name}`}
                className="p-2 rounded-lg text-slate-400 hover:text-white hover:bg-slate-700 transition-colors shrink-0 ml-2"
              >
                <Download className="w-4 h-4" />
              </a>
            </div>
          ))
        )}
      </div>
    </div>
  );
};
