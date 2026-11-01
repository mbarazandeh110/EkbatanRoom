/** @type {import('next').NextConfig} */
const nextConfig = {
  output: 'standalone',
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
