"use client";

import React from "react";
import { Server, AlertTriangle, ShieldCheck, ShieldAlert, Network } from "lucide-react";

export interface ClusterLicenseInfo {
  sfuTopology: "single-node" | "cascaded-cluster";
  activeMediaNode: string;
  totalNodesInCluster?: number;
  licensedCapacity: number;
  currentParticipants: number;
  licenseExpiresAt?: string;
  isEnterprise: boolean;
}

export interface ClusterLicenseBannerProps {
  info: ClusterLicenseInfo;
  isHost?: boolean;
}

export const ClusterLicenseBanner: React.FC<ClusterLicenseBannerProps> = ({
  info,
  isHost = false,
}) => {
  const usagePercentage = Math.round(
    (info.currentParticipants / Math.max(1, info.licensedCapacity)) * 100
  );

  const isExceeded = info.currentParticipants >= info.licensedCapacity;
  const isNearLimit = usagePercentage >= 85 && !isExceeded;

  return (
    <div
      data-testid="cluster-license-banner"
      className="w-full bg-slate-950 border-b border-slate-800 px-4 py-1.5 flex items-center justify-between text-xs select-none flex-wrap gap-2"
    >
      {/* SFU Topology & Cluster Node Info */}
      <div className="flex items-center space-x-3 text-slate-400">
        <div className="flex items-center space-x-1.5">
          <Network className="w-3.5 h-3.5 text-blue-400" />
          <span className="font-medium text-slate-300">
            {info.sfuTopology === "cascaded-cluster"
              ? `Cascaded Cluster (${info.totalNodesInCluster || 2} SFU Nodes)`
              : "Single-Node Media SFU"}
          </span>
        </div>

        <span className="hidden sm:inline text-slate-600">•</span>

        <div className="hidden sm:flex items-center space-x-1 font-mono text-[11px] text-slate-400">
          <Server className="w-3 h-3 text-slate-500" />
          <span>{info.activeMediaNode}</span>
        </div>
      </div>

      {/* License Capacity & Quota Alert */}
      <div className="flex items-center space-x-3">
        {/* Warning if capacity exceeded or near limit */}
        {isExceeded ? (
          <div className="flex items-center space-x-1 text-red-400 font-semibold text-[11px] bg-red-950/50 px-2 py-0.5 rounded border border-red-800">
            <ShieldAlert className="w-3.5 h-3.5 animate-pulse" />
            <span>Room Capacity Limit Reached ({info.currentParticipants}/{info.licensedCapacity})</span>
          </div>
        ) : isNearLimit ? (
          <div className="flex items-center space-x-1 text-amber-400 font-medium text-[11px] bg-amber-950/40 px-2 py-0.5 rounded border border-amber-800/60">
            <AlertTriangle className="w-3.5 h-3.5" />
            <span>License Quota High ({usagePercentage}%)</span>
          </div>
        ) : (
          <div className="flex items-center space-x-1 text-slate-400 text-[11px]">
            <ShieldCheck className="w-3.5 h-3.5 text-emerald-400" />
            <span>
              Capacity: <strong className="text-slate-200">{info.currentParticipants}</strong> / {info.licensedCapacity}
            </span>
          </div>
        )}

        {/* Enterprise Badge */}
        {info.isEnterprise && (
          <span className="px-2 py-0.5 rounded-full text-[10px] font-semibold bg-emerald-500/10 text-emerald-400 border border-emerald-500/30">
            Enterprise Tier
          </span>
        )}

        {isHost && (
          <span className="px-2 py-0.5 rounded-full text-[10px] font-medium bg-slate-800 text-slate-300 border border-slate-700">
            Host Admin
          </span>
        )}
      </div>
    </div>
  );
};
