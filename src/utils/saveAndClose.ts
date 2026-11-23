export async function saveAndClose(
    save: () => Promise<boolean>,
    close: () => void | Promise<void>,
    dismiss: () => void,
) {
    if (await save()) {
        await close();
        dismiss();
    }
}
