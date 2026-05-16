import type { Metadata } from "next";
import { cookies } from "next/headers";
import Link from "next/link";
import { Geist, Geist_Mono } from "next/font/google";
import "./globals.css";
import { ApolloClientProvider } from "@/lib/apollo-provider";
import { AuthProvider } from "@/lib/auth-context";
import { LogoutButton } from "@/components/layout/logout-button";
import { PrimaryNav } from "@/components/layout/nav";
import { CookieBanner } from "@/components/layout/cookie-banner";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});

export const metadata: Metadata = {
  title: "JudicialPredict Workspace",
  description: "AI-powered case evaluation platform for law firms.",
};

export default async function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  // Read jp_session server-side so we can conditionally render the nav bar.
  // This does NOT verify the JWT — gateway handles that on every API request.
  const cookieStore = await cookies();
  const isAuthenticated = cookieStore.has("jp_session");

  return (
    <html lang="en">
      <body
        className={`${geistSans.variable} ${geistMono.variable} antialiased`}
      >
        <ApolloClientProvider>
          <AuthProvider>
            {isAuthenticated && (
              <header className="flex flex-wrap items-center justify-between gap-4 border-b px-6 py-3">
                <div className="flex items-center gap-6">
                  <Link
                    href="/cases"
                    className="text-sm font-semibold tracking-tight hover:text-primary"
                  >
                    JudicialPredict
                  </Link>
                  <PrimaryNav />
                </div>
                <LogoutButton />
              </header>
            )}
            {children}
            <CookieBanner />
          </AuthProvider>
        </ApolloClientProvider>
      </body>
    </html>
  );
}
