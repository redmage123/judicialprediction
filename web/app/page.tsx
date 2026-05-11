import { cookies } from "next/headers";
import { redirect } from "next/navigation";
import Link from "next/link";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";

// Authenticated operators land on the case list. Unauthenticated visitors get
// a short marketing card with a sign-in CTA. The internal /healthz check is
// out of band (docker compose ps / scripts/jp-smoke) — it does not belong on
// the operator-facing home.
export default async function HomePage() {
  const cookieStore = await cookies();
  if (cookieStore.has("jp_session")) {
    redirect("/cases");
  }

  return (
    <main className="flex min-h-screen items-center justify-center p-8">
      <Card className="w-full max-w-md text-center">
        <CardHeader>
          <CardTitle className="text-2xl font-bold tracking-tight">
            JudicialPredict
          </CardTitle>
          <CardDescription>
            AI-powered case evaluation for law firms.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Button asChild className="w-full">
            <Link href="/login">Sign in</Link>
          </Button>
        </CardContent>
      </Card>
    </main>
  );
}
