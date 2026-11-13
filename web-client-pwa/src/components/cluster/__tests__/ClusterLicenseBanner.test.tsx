import React from "react";
import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { ClusterLicenseBanner, ClusterLicenseInfo } from "../ClusterLicenseBanner";

describe("ClusterLicenseBanner Component", () => {
  it("renders single-node info and capacity within normal range", () => {
    const info: ClusterLicenseInfo = {
      sfuTopology: "single-node",
      activeMediaNode: "sfu-tehran-01",
      licensedCapacity: 100,
      currentParticipants: 25,
      isEnterprise: true,
    };

    render(<ClusterLicenseBanner info={info} />);

    expect(screen.getByTestId("cluster-license-banner")).toBeInTheDocument();
    expect(screen.getByText("Single-Node Media SFU")).toBeInTheDocument();
    expect(screen.getByText("sfu-tehran-01")).toBeInTheDocument();
    expect(screen.getByText(/25/)).toBeInTheDocument();
    expect(screen.getByText("Enterprise Tier")).toBeInTheDocument();
  });

  it("renders cascaded cluster info and warning when capacity is exceeded", () => {
    const info: ClusterLicenseInfo = {
      sfuTopology: "cascaded-cluster",
      activeMediaNode: "sfu-cluster-edge-02",
      totalNodesInCluster: 4,
      licensedCapacity: 50,
      currentParticipants: 50,
      isEnterprise: false,
    };

    render(<ClusterLicenseBanner info={info} isHost={true} />);

    expect(
      screen.getByText("Cascaded Cluster (4 SFU Nodes)")
    ).toBeInTheDocument();
    expect(
      screen.getByText(/room capacity limit reached/i)
    ).toBeInTheDocument();
  });
});
