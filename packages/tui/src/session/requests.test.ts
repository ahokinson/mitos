import { expect, test } from "bun:test";

import {
  describeRequest,
  RequestKind,
  requestHint,
} from "@session/requests.ts";

test("requests describe themselves by kind and title", () => {
  expect(describeRequest(RequestKind.Permission, { title: " Bash ls " })).toBe(
    "Permission: Bash ls",
  );
  expect(describeRequest(RequestKind.Question, {})).toBe(
    "Question: (no details)",
  );
  expect(describeRequest(RequestKind.PlanApproval, { title: "  " })).toBe(
    "Plan ready: (no details)",
  );
  expect(describeRequest(RequestKind.Question, null)).toBe(
    "Question: (no details)",
  );
  expect(describeRequest(RequestKind.Question, "text")).toBe(
    "Question: (no details)",
  );
});

test("every request kind hints at its commands", () => {
  expect(requestHint(RequestKind.Question)).toContain("/answer");
  expect(requestHint(RequestKind.Permission)).toContain("/approve");
  expect(requestHint(RequestKind.PlanApproval)).toContain("/deny");
});
