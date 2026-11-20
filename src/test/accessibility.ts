import axe from "axe-core";
import { expect } from "vitest";

export async function expectAccessible(root: Element = document.body) {
  const result = await axe.run(root, {
    rules: {
      "color-contrast": { enabled: false },
      "aria-dialog-name": { enabled: true },
      // Views are mounted inside App's main landmark in production.
      region: { enabled: false },
    },
  });
  expect(result.violations.map(({ id, nodes }) => ({ id, targets: nodes.map((node) => node.target) }))).toEqual([]);
}
