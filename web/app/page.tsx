import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";

const GATEWAY_INTERNAL =
  process.env.GATEWAY_INTERNAL_URL ?? "http://localhost:4000";

interface HealthzResponse {
  status: string;
  timestamp?: string;
  version?: string;
  [key: string]: unknown;
}

async function fetchHealthz(): Promise<HealthzResponse | null> {
  try {
    const res = await fetch(`${GATEWAY_INTERNAL}/healthz`, {
      // No caching — fresh check on each request during dev.
      cache: "no-store",
    });
    if (!res.ok) return null;
    return res.json() as Promise<HealthzResponse>;
  } catch {
    return null;
  }
}

export default async function HomePage() {
  const health = await fetchHealthz();
  const isHealthy = health?.status === "ok" || health?.status === "healthy";

  return (
    <main className="flex min-h-screen items-center justify-center p-8">
      <Card className="w-full max-w-md" aria-label="API health status">
        <CardHeader>
          <CardTitle className="text-2xl font-bold tracking-tight">
            JudicialPredict
          </CardTitle>
          <CardDescription>API gateway health check</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          {health ? (
            <>
              <div
                className="flex items-center gap-3"
                aria-live="polite"
                aria-atomic="true"
              >
                <span
                  className={`inline-block h-3 w-3 rounded-full ${
                    isHealthy ? "bg-green-500" : "bg-yellow-500"
                  }`}
                  aria-hidden="true"
                />
                <span className="text-lg font-semibold">
                  {isHealthy ? "Healthy" : health.status}
                </span>
              </div>
              {health.timestamp && (
                <p className="text-sm text-muted-foreground">
                  <span className="font-medium">Timestamp: </span>
                  <time dateTime={health.timestamp}>{health.timestamp}</time>
                </p>
              )}
              {health.version && (
                <p className="text-sm text-muted-foreground">
                  <span className="font-medium">Version: </span>
                  {health.version}
                </p>
              )}
            </>
          ) : (
            <p className="text-sm text-red-600" role="alert" aria-live="assertive">
              Unable to reach api-gateway at{" "}
              <code className="font-mono">{GATEWAY_INTERNAL}/healthz</code>.
              Start the api-gateway service and reload.
            </p>
          )}
        </CardContent>
      </Card>
    </main>
  );
}
