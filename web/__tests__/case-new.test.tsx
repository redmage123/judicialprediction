/**
 * case-new tests — S3.2
 *
 * Covers:
 *  1. All 7 fields render with accessible labels
 *  2. Happy path: mutation resolves, router.push called with /case/<uuid>
 *  3. Validation: out-of-range value prevents submit and shows inline error
 *  4. GraphQL error path: inline alert, no redirect
 *  5. Network error path: generic alert, no redirect
 *  6. axe-core a11y gate on the form
 */

import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { axe, toHaveNoViolations } from "jest-axe";

// ---------------------------------------------------------------------------
// Hoist spies (must come before vi.mock factories)
// ---------------------------------------------------------------------------

const { mockRouterPush, mockMutate } = vi.hoisted(() => ({
  mockRouterPush: vi.fn(),
  mockMutate: vi.fn(),
}));

// ---------------------------------------------------------------------------
// Mocks
// ---------------------------------------------------------------------------

vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: mockRouterPush }),
}));

/**
 * Mock @apollo/client/react — useMutation lives here in Apollo Client v4.
 * The real gql tag (in @apollo/client root) is NOT mocked so predict.ts
 * can parse the GraphQL document at import time without issues.
 */
vi.mock("@apollo/client/react", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@apollo/client/react")>();
  return {
    ...actual,
    useMutation: () => [mockMutate, { loading: false, error: undefined, data: null }],
  };
});

// ---------------------------------------------------------------------------
// Extend matchers
// ---------------------------------------------------------------------------

expect.extend(toHaveNoViolations);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async function renderForm() {
  // Dynamic import ensures mocks above are applied first.
  const { IntakeForm } = await import("../app/case/new/intake-form");
  return render(<IntakeForm />);
}

/** Fill all 7 fields with valid values. */
function fillAllFields() {
  fireEvent.change(screen.getByLabelText(/judge severity/i), { target: { value: "0.65" } });
  fireEvent.change(screen.getByLabelText(/attorney win rate/i), { target: { value: "0.72" } });
  fireEvent.change(screen.getByLabelText(/ideology distance/i), { target: { value: "0.41" } });
  fireEvent.change(screen.getByLabelText(/materiality score/i), { target: { value: "0.88" } });
  fireEvent.change(screen.getByLabelText(/procedural motions filed/i), { target: { value: "3" } });
  // selects already have default values ("civil" / "us-federal"), so no fireEvent needed
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("IntakeForm — field rendering", () => {
  beforeEach(() => vi.clearAllMocks());

  it("renders all 7 fields with accessible labels", async () => {
    await renderForm();

    expect(screen.getByLabelText(/judge severity/i)).toBeTruthy();
    expect(screen.getByLabelText(/attorney win rate/i)).toBeTruthy();
    expect(screen.getByLabelText(/ideology distance/i)).toBeTruthy();
    expect(screen.getByLabelText(/materiality score/i)).toBeTruthy();
    expect(screen.getByLabelText(/procedural motions filed/i)).toBeTruthy();
    expect(screen.getByLabelText(/case type/i)).toBeTruthy();
    expect(screen.getByLabelText(/jurisdiction/i)).toBeTruthy();
  });
});

describe("IntakeForm — happy path", () => {
  beforeEach(() => vi.clearAllMocks());

  it("calls the mutation and routes to /case/<uuid> on success", async () => {
    mockMutate.mockResolvedValue({
      data: {
        predictCaseOutcome: {
          pWin: 0.74,
          ciLower: 0.62,
          ciUpper: 0.86,
          coverage: 0.95,
          modelVersion: "tier-ab-v1.0",
          predictedAtUnix: 1715000000,
        },
      },
      errors: undefined,
    });

    await renderForm();
    fillAllFields();

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /run prediction/i }));
    });

    await waitFor(() => {
      expect(mockMutate).toHaveBeenCalledOnce();
    });

    const routeArg: string = mockRouterPush.mock.calls[0]?.[0] ?? "";
    expect(routeArg).toMatch(/^\/case\/[0-9a-f-]{36}$/);
  });
});

describe("IntakeForm — validation", () => {
  beforeEach(() => vi.clearAllMocks());

  it("shows inline error and does not call mutation when judgeSeverity is out of range", async () => {
    await renderForm();

    // Enter an out-of-range value (1.5 > 1)
    fireEvent.change(screen.getByLabelText(/judge severity/i), { target: { value: "1.5" } });
    fireEvent.change(screen.getByLabelText(/attorney win rate/i), { target: { value: "0.5" } });
    fireEvent.change(screen.getByLabelText(/ideology distance/i), { target: { value: "0.5" } });
    fireEvent.change(screen.getByLabelText(/materiality score/i), { target: { value: "0.5" } });
    fireEvent.change(screen.getByLabelText(/procedural motions filed/i), { target: { value: "5" } });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /run prediction/i }));
    });

    // Inline field error should appear
    await waitFor(() => {
      expect(screen.getAllByRole("alert").length).toBeGreaterThan(0);
    });
    expect(screen.getByText(/must be between 0 and 1/i)).toBeTruthy();

    // Mutation must NOT have been called
    expect(mockMutate).not.toHaveBeenCalled();
  });
});

describe("IntakeForm — GraphQL error path", () => {
  beforeEach(() => vi.clearAllMocks());

  it("shows inline alert and does NOT redirect on GraphQL error", async () => {
    mockMutate.mockResolvedValue({
      data: null,
      error: { message: "Prediction model unavailable" },
    });

    await renderForm();
    fillAllFields();

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /run prediction/i }));
    });

    await waitFor(() => {
      expect(screen.getByRole("alert")).toBeTruthy();
    });
    expect(screen.getByText(/prediction model unavailable/i)).toBeTruthy();
    expect(mockRouterPush).not.toHaveBeenCalled();
  });
});

describe("IntakeForm — network error path", () => {
  beforeEach(() => vi.clearAllMocks());

  it("shows generic alert and does NOT redirect on network error", async () => {
    mockMutate.mockRejectedValue(new Error("fetch failed"));

    await renderForm();
    fillAllFields();

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /run prediction/i }));
    });

    await waitFor(() => {
      expect(screen.getByRole("alert")).toBeTruthy();
    });
    expect(screen.getByText(/unable to reach the gateway/i)).toBeTruthy();
    expect(mockRouterPush).not.toHaveBeenCalled();
  });
});

describe("IntakeForm — a11y gate", () => {
  it("passes axe-core with no violations", async () => {
    const { container } = await renderForm();
    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });
});
