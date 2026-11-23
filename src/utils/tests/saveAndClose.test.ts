import { expect, test, vi } from "vitest";
import { saveAndClose } from "../saveAndClose";

test("tab stays open while saving is pending", async () => {
	let finishSave!: (saved: boolean) => void;
	const save = () =>
		new Promise<boolean>((resolve) => {
			finishSave = resolve;
		});
	const close = vi.fn(async () => {});
	const dismiss = vi.fn();

	const task = saveAndClose(save, close, dismiss);

	expect(close).not.toHaveBeenCalled();
	expect(dismiss).not.toHaveBeenCalled();

	finishSave(true);
	await task;

	expect(close).toHaveBeenCalledOnce();
	expect(dismiss).toHaveBeenCalledOnce();
});

test("canceling a save leaves the tab open", async () => {
	const close = vi.fn(async () => {});
	const dismiss = vi.fn();

	await saveAndClose(async () => false, close, dismiss);

	expect(close).not.toHaveBeenCalled();
	expect(dismiss).not.toHaveBeenCalled();
});

test("failed save does not close the tab", async () => {
	const close = vi.fn(async () => {});
	const dismiss = vi.fn();

	await expect(
		saveAndClose(
			async () => {
				throw new Error("write failed");
			},
			close,
			dismiss,
		),
	).rejects.toThrow("write failed");

	expect(close).not.toHaveBeenCalled();
	expect(dismiss).not.toHaveBeenCalled();
});

test("tab cleanup finishes before dismissing the modal", async () => {
	let finishClose!: () => void;
	const close = vi.fn(
		() =>
			new Promise<void>((resolve) => {
				finishClose = resolve;
			}),
	);
	const dismiss = vi.fn();

	const task = saveAndClose(async () => true, close, dismiss);
	await Promise.resolve();

	expect(close).toHaveBeenCalledOnce();
	expect(dismiss).not.toHaveBeenCalled();

	finishClose();
	await task;

	expect(dismiss).toHaveBeenCalledOnce();
});

test("failed tab cleanup does not dismiss the modal", async () => {
	const dismiss = vi.fn();

	await expect(
		saveAndClose(
			async () => true,
			async () => {
				throw new Error("cleanup failed");
			},
			dismiss,
		),
	).rejects.toThrow("cleanup failed");

	expect(dismiss).not.toHaveBeenCalled();
});
