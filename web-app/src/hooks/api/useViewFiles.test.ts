// @vitest-environment jsdom
import { renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import useViewFiles from "./useViewFiles";

vi.mock("./files/useFileTree", () => ({
  default: () => ({
    data: {
      primary: [
        {
          name: "semantics",
          path: "semantics",
          is_dir: true,
          children: [
            {
              name: "orders.view.yml",
              path: "semantics/views/orders.view.yml",
              is_dir: false,
              children: []
            },
            {
              name: "sales.topic.yml",
              path: "semantics/topics/sales.topic.yml",
              is_dir: false,
              children: []
            }
          ]
        }
      ]
    },
    isLoading: false,
    error: null
  })
}));

describe("useViewFiles", () => {
  it("flattens only .view.yml files", () => {
    const { result } = renderHook(() => useViewFiles());
    expect(result.current.viewFiles).toHaveLength(1);
    expect(result.current.viewFiles[0]).toMatchObject({
      value: "orders",
      label: "orders",
      path: "semantics/views/orders.view.yml"
    });
  });
});
