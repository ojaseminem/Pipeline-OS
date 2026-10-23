import { onlineManager } from "@tanstack/react-query";
import { afterEach, describe, expect, it } from "vitest";
import { createQueryClient } from "./queries";

describe("query client offline", () => {
  afterEach(() => onlineManager.setOnline(true));

  it("still runs local queries and mutations when the OS reports no network", async () => {
    onlineManager.setOnline(false);
    const client = createQueryClient();
    await expect(client.fetchQuery({ queryKey: ["assets"], queryFn: async () => ["hero.png"] })).resolves.toEqual(["hero.png"]);
    const mutation = client.getMutationCache().build(client, { mutationFn: async (title: string) => title.toUpperCase() });
    await expect(mutation.execute("plan")).resolves.toBe("PLAN");
  });
});
