import type { Metadata } from "next";
import { ForgotPasswordForm } from "./forgot-password-form";

export const metadata: Metadata = {
  title: "Reset password — JudicialPredict",
};

export default function ForgotPasswordPage() {
  return (
    <main className="flex min-h-screen items-center justify-center p-8">
      <h1 className="sr-only">Forgot password — JudicialPredict</h1>
      <ForgotPasswordForm />
    </main>
  );
}
