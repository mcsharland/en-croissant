import { save as chooseFile } from "@tauri-apps/plugin-dialog";
import type { Dispatch, SetStateAction } from "react";
import { beforeEach, expect, test, vi } from "vitest";
import type { StoreApi } from "zustand";
import { commands } from "@/bindings";
import type { TreeStoreState } from "@/state/store/tree";
import { type GameOrigin, saveToFile, type Tab } from "../tabs";
import { defaultTree } from "../treeReducer";

vi.mock("@/bindings", () => ({
    commands: {
        writeGame: vi.fn(),
        writeDbGame: vi.fn(),
    },
}));

vi.mock("@tauri-apps/api/path", () => ({
    resolve: vi.fn(async (...parts: string[]) => parts.join("/")),
    tempDir: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
    save: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-fs", () => ({
    copyFile: vi.fn(),
}));

function setup(origin: GameOrigin = { kind: "none" }) {
    const tab: Tab = {
        name: "First",
        value: "first",
        type: "analysis",
        gameOrigin: origin,
    };
    const other: Tab = {
        name: "Second",
        value: "second",
        type: "analysis",
        gameOrigin: { kind: "none" },
    };
    let tabs = [tab, other];

    const setTabs: Dispatch<SetStateAction<Tab[]>> = (update) => {
        tabs = typeof update === "function" ? update(tabs) : update;
    };

    const tree = defaultTree();
    tree.dirty = true;
    const markSaved = vi.fn(() => {
        tree.dirty = false;
    });
    const store = {
        getState: () => ({ ...tree, save: markSaved }),
    } as unknown as StoreApi<TreeStoreState>;

    return { tab, other, setTabs, store, markSaved, getTabs: () => tabs };
}

beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(chooseFile).mockResolvedValue("/docs/saved.pgn");
    vi.mocked(commands.writeGame).mockResolvedValue({ status: "ok", data: null });
    vi.mocked(commands.writeDbGame).mockResolvedValue({
        status: "ok",
        data: null,
    });
});

test("canceling Save As does not write or clear changes", async () => {
    const { tab, setTabs, store, markSaved, getTabs } = setup();
    vi.mocked(chooseFile).mockResolvedValue(null);

    const saved = await saveToFile({
        dir: "/docs",
        tab,
        setTabs,
        store,
        isUserSave: true,
    });

    expect(saved).toBe(false);
    expect(commands.writeGame).not.toHaveBeenCalled();
    expect(markSaved).not.toHaveBeenCalled();
    expect(store.getState().dirty).toBe(true);
    expect(getTabs()[0]).toEqual(tab);
});

test("failed file write preserves the origin and dirty state", async () => {
    const { tab, setTabs, store, markSaved, getTabs } = setup();
    vi.mocked(commands.writeGame).mockRejectedValue(new Error("write failed"));

    await expect(
        saveToFile({
            dir: "/docs",
            tab,
            setTabs,
            store,
            isUserSave: true,
        }),
    ).rejects.toThrow("write failed");

    expect(markSaved).not.toHaveBeenCalled();
    expect(store.getState().dirty).toBe(true);
    expect(getTabs()[0]).toEqual(tab);
});

test("tab origin changes only after the file write finishes", async () => {
    const { tab, other, setTabs, store, markSaved, getTabs } = setup();

    let finishWrite!: () => void;
    const pending = new Promise<Awaited<ReturnType<typeof commands.writeGame>>>((resolve) => {
        finishWrite = () => resolve({ status: "ok", data: null });
    });
    vi.mocked(commands.writeGame).mockReturnValue(pending);

    const saving = saveToFile({
        dir: "/docs",
        tab,
        setTabs,
        store,
        isUserSave: true,
    });

    await vi.waitFor(() => expect(commands.writeGame).toHaveBeenCalledOnce());

    expect(getTabs()[0]).toEqual(tab);
    expect(markSaved).not.toHaveBeenCalled();
    expect(store.getState().dirty).toBe(true);

    finishWrite();
    await expect(saving).resolves.toBe(true);

    expect(getTabs()[0].gameOrigin).toMatchObject({
        kind: "file",
        file: { path: "/docs/saved.pgn" },
    });
    expect(getTabs()[1]).toEqual(other);
    expect(markSaved).toHaveBeenCalledOnce();
    expect(store.getState().dirty).toBe(false);
});

test("saving an existing file does not open Save As", async () => {
    const origin: GameOrigin = {
        kind: "file",
        gameNumber: 2,
        file: {
            type: "file",
            name: "existing.pgn",
            path: "/docs/existing.pgn",
            numGames: 3,
            metadata: { tags: [], type: "game" },
            lastModified: 0,
        },
    };
    const { tab, setTabs, store, markSaved, getTabs } = setup(origin);

    const saved = await saveToFile({ dir: "/docs", tab, setTabs, store });

    expect(saved).toBe(true);
    expect(chooseFile).not.toHaveBeenCalled();
    expect(commands.writeGame).toHaveBeenCalledWith("/docs/existing.pgn", 2, expect.any(String));
    expect(getTabs()[0]).toEqual(tab);
    expect(markSaved).toHaveBeenCalledOnce();
});

test("database save marks the game clean after writing", async () => {
    const { tab, setTabs, store, markSaved } = setup({
        kind: "database",
        database: "games.db",
        gameId: 12,
    });

    const saved = await saveToFile({ dir: "/docs", tab, setTabs, store });

    expect(saved).toBe(true);
    expect(commands.writeDbGame).toHaveBeenCalledWith("games.db", 12, expect.any(String));
    expect(commands.writeGame).not.toHaveBeenCalled();
    expect(markSaved).toHaveBeenCalledOnce();
});
