import type { Metadata } from "next";
import { ResetPasswordForm } from "./reset-password-form";

export const metadata: Metadata = {
  title: "Set a new password — JudicialPredict",
};

/**
 * /reset-password?token=... — the link emailed by the request flow.
 * The token is in the query string; this server component pulls it out and
 * passes it to the client form.  An empty/missing token still renders the
 * form (the confirm endpoint will reject it), so the page works even when
 * users hand-edit the URL.
 */
export default async function ResetPasswordPage({
  searchParams,
}: {
  searchParams: Promise<{ token?: string }>;
}) {
  const { token } = await searchParams;
  return (
    <main className="flex min-h-screen items-center justify-center p-8">
      <ResetPasswordForm token={token ?? ""} />
    </main>
  );
}
