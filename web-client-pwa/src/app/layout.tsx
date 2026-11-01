import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "EkbatanRoom WebRTC Conference",
  description: "Enterprise Dual-Protocol WebRTC Video Conference PWA",
  manifest: "/manifest.json",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <body className="bg-slate-950 text-slate-100 antialiased min-h-screen">
        {children}
      </body>
    </html>
  );
}
