import type { Metadata } from "next";
import { Suspense } from "react";
import { ThemeProvider } from "@/components/ThemeProvider";
import ProgressBar from "@/components/ProgressBar";
import { OnboardingTour } from "@/components/OnboardingTour";
import NavHeader from "@/components/NavHeader";
import { ToastProvider } from "@/components/toast";
import { NotificationProvider } from "@/components/notifications";
import { PwaInstallPrompt } from "@/components/PwaInstallPrompt";
import { ServiceWorkerRegistration } from "@/components/ServiceWorkerRegistration";
import {
  FreighterNetworkBanner,
  FreighterNetworkProvider,
} from "@/components/FreighterNetworkBanner";
import "./globals.css";

export const metadata: Metadata = {
  title: "Aura Vault Protocol",
  description: "Share-based yield vault on Stellar — deposit, compound, and withdraw with Freighter.",
  manifest: "/manifest.json",
  themeColor: "#6366f1",
  appleWebApp: {
    capable: true,
    title: "Aura Vault",
    statusBarStyle: "black-translucent",
  },
};

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en" dir="ltr" className="h-full antialiased" suppressHydrationWarning>
      <body className="min-h-full flex flex-col bg-white text-zinc-900 transition-colors duration-200 dark:bg-zinc-950 dark:text-zinc-100">
        <ThemeProvider>
          <ToastProvider>
            <NotificationProvider>
              <FreighterNetworkProvider>
                <Suspense fallback={null}><ProgressBar /></Suspense>
                <OnboardingTour />
                <FreighterNetworkBanner />
                <NavHeader />
                <main className="flex-1">{children}</main>
                <PwaInstallPrompt />
                <ServiceWorkerRegistration />
              </FreighterNetworkProvider>
            </NotificationProvider>
          </ToastProvider>
        </ThemeProvider>
      </body>
    </html>
  );
}
