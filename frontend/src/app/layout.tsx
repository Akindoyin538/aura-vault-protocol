import type { Metadata } from "next";
import { Suspense } from "react";

import { FreighterNetworkBanner, FreighterNetworkProvider } from "@/components/FreighterNetworkBanner";
import LanguageSwitcher from "@/components/LanguageSwitcher";
import NavHeader from "@/components/NavHeader";
import { NotificationCenter, NotificationProvider } from "@/components/notifications";
import { OnboardingTour } from "@/components/OnboardingTour";
import ProgressBar from "@/components/ProgressBar";
import { ServiceWorkerRegistration } from "@/components/ServiceWorkerRegistration";
import { ThemeProvider } from "@/components/ThemeProvider";
import { ThemeToggle } from "@/components/ThemeToggle";
import { ToastProvider } from "@/components/toast";
import "./globals.css";

export const metadata: Metadata = {
  title: "Aura Vault Protocol",
  description: "Share-based yield vault on Stellar — deposit, compound, and withdraw with Freighter.",
  manifest: "/manifest.json",
  themeColor: "#6366f1",
  appleWebApp: { capable: true, title: "Aura Vault", statusBarStyle: "black-translucent" },
};

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en" dir="ltr" className="h-full antialiased" suppressHydrationWarning>
      <body className="min-h-full flex flex-col bg-white text-zinc-900 transition-colors duration-200 dark:bg-zinc-950 dark:text-zinc-100">
        <ThemeProvider>
          <Suspense fallback={null}>
            <ProgressBar />
          </Suspense>
          <OnboardingTour />
          <ToastProvider>
            <NotificationProvider>
              <FreighterNetworkProvider>
                <FreighterNetworkBanner />
                <a
                  href="#main-content"
                  className="sr-only z-50 rounded-md bg-white px-4 py-3 text-black focus:not-sr-only focus:fixed focus:left-4 focus:top-4 focus:outline focus:outline-2 focus:outline-offset-2 focus:outline-blue-700"
                >
                  Skip to main content
                </a>
                <div className="relative">
                  <NavHeader />
                  <div className="absolute right-4 top-3 z-40 flex items-center gap-3">
                    <LanguageSwitcher />
                    <ThemeToggle />
                    <NotificationCenter />
                  </div>
                </div>
                <main id="main-content" tabIndex={-1} className="flex flex-1 flex-col">
                  {children}
                </main>
                <ServiceWorkerRegistration />
              </FreighterNetworkProvider>
            </NotificationProvider>
          </ToastProvider>
        </ThemeProvider>
      </body>
    </html>
  );
}
