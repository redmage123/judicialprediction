/**
 * CasesTable stories — S4.5 (JP-59)
 *
 * Two baseline stories:
 *  - Loaded: a page of 3 cases with mixed recommendations and pagination.
 *  - Empty:  totalCount=0, shows the "No cases yet" CTA.
 *
 * next/link is stubbed via the decorator so navigation links render as plain
 * <a> tags in the Storybook canvas.
 */

import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { CasesTable } from "../app/cases/cases-table";
import type { CaseConnection } from "../lib/queries/predict";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const LOADED: CaseConnection = {
  nodes: [
    {
      id: "aaaaaaaa-0000-0000-0000-000000000001",
      inputFeatures: { caseType: "civil", jurisdiction: "us-federal" },
      prediction: { pWin: 0.72 },
      recommendation: { kind: "Try" },
      createdAt: "2026-05-09T10:00:00Z",
      dateFiled: null,
      createdBy: null,
    },
    {
      id: "aaaaaaaa-0000-0000-0000-000000000002",
      inputFeatures: { caseType: "criminal", jurisdiction: "ca-state" },
      prediction: { pWin: 0.38 },
      recommendation: { kind: "Settle" },
      createdAt: "2026-05-08T09:30:00Z",
      dateFiled: null,
      createdBy: null,
    },
    {
      id: "aaaaaaaa-0000-0000-0000-000000000003",
      inputFeatures: { caseType: "bankruptcy", jurisdiction: "nj-state" },
      prediction: { pWin: 0.5 },
      recommendation: { kind: "Borderline" },
      createdAt: "2026-05-07T14:00:00Z",
      dateFiled: null,
      createdBy: null,
    },
  ],
  totalCount: 47,
  nextOffset: 20,
};

const EMPTY: CaseConnection = {
  nodes: [],
  totalCount: 0,
  nextOffset: null,
};

// ---------------------------------------------------------------------------
// Meta
// ---------------------------------------------------------------------------

const meta: Meta<typeof CasesTable> = {
  title: "JudicialPredict/CasesTable",
  component: CasesTable,
  parameters: {
    layout: "fullscreen",
  },
  tags: ["autodocs"],
};

export default meta;
type Story = StoryObj<typeof meta>;

// ---------------------------------------------------------------------------
// Stories
// ---------------------------------------------------------------------------

export const Loaded: Story = {
  args: {
    connection: LOADED,
    offset: 0,
    pageSize: 20,
  },
};

export const SecondPage: Story = {
  args: {
    connection: {
      ...LOADED,
      nextOffset: null, // last page — Next disabled
    },
    offset: 20,
    pageSize: 20,
  },
};

export const Empty: Story = {
  args: {
    connection: EMPTY,
    offset: 0,
    pageSize: 20,
  },
};
