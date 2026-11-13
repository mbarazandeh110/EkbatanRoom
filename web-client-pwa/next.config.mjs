import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

/** @type {import('next').NextConfig} */
const nextConfig = {
  output: 'standalone',
  outputFileTracingRoot: __dirname,
  reactStrictMode: true,
  async rewrites() {
    const livekitUrl =
      process.env.LIVEKIT_HTTP_URL ||
      process.env.NEXT_PUBLIC_LIVEKIT_HTTP_URL ||
      "http://localhost:7880";
    return [
      {
        source: "/rtc/:path*",
        destination: `${livekitUrl}/rtc/:path*`,
      },
    ];
  },
};

export default nextConfig;
