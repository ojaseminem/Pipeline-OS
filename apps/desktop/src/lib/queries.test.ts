import { onlineManager } from "@tanstack/react-query";
import { afterEach, describe, expect, it } from "vitest";
import { createQueryClient } from "./queries";

describe("query client offline", () => {
  afterEach(() => onlineManager.setOnline(true));

  it("still loads and acts when the OS reports no network", async () => {
    onlineManager.setOnline(false);
    const client = createQueryClient();

    // Local IPC reads must not pause waiting for a network.
    await expect(client.fetchQuery({ queryKey: ["local"], queryFn: async () => "projects" })).resolves.toBe("projects");

    // Neither may local actions.
    const action = client.getMutationCache().build(client, { mutationFn: async (name: string) => `saved ${name}` });
    await expect(action.execute("task")).resolves.toBe("saved task");
  });
});
