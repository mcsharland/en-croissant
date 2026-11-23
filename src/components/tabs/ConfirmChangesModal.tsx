import { Button, Group, Modal, Stack, Text } from "@mantine/core";
import { useLoaderData } from "@tanstack/react-router";
import { useSetAtom } from "jotai";
import { useContext, useRef, useState } from "react";
import { tabsAtom } from "@/state/atoms";
import { saveAndClose } from "@/utils/saveAndClose";
import { saveToFile, type Tab } from "@/utils/tabs";
import { TreeStateContext } from "../common/TreeStateContext";

function ConfirmChangesModal({
  tab,
  opened,
  onCancel,
  closeTab,
}: {
  tab: Tab;
  opened: boolean;
  onCancel: () => void;
  closeTab: () => void | Promise<void>;
}) {
  const setTabs = useSetAtom(tabsAtom);
  const store = useContext(TreeStateContext)!;
  const { documentDir } = useLoaderData({ from: "/" });

  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const savingRef = useRef(false);

  async function handleSaveAndClose() {
    if (savingRef.current) return;

    savingRef.current = true;
    setSaving(true);
    setError(null);

    try {
      await saveAndClose(
        () =>
          saveToFile({
            dir: documentDir,
            tab,
            setTabs,
            store,
            isUserSave: true,
          }),
        closeTab,
        onCancel,
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      savingRef.current = false;
      setSaving(false);
    }
  }

  async function handleDiscard() {
    if (savingRef.current) return;

    try {
      await closeTab();
      onCancel();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  return (
    <Modal
      withCloseButton={false}
      opened={opened}
      onClose={() => {
        if (!savingRef.current) onCancel();
      }}
      closeOnEscape={!saving}
      closeOnClickOutside={!saving}
    >
      <Stack>
        <div>
          <Text fz="lg" fw="bold" mb={10}>
            Unsaved Changes
          </Text>
          <Text>You have unsaved changes. Do you want to save them before closing?</Text>
        </div>

        {error && (
          <Text c="red" size="sm">
            {error}
          </Text>
        )}

        <Group justify="right">
          <Button variant="default" disabled={saving} onClick={handleDiscard}>
            Close Without Saving
          </Button>
          <Button loading={saving} disabled={saving} onClick={handleSaveAndClose}>
            Save and Close
          </Button>
        </Group>
      </Stack>
    </Modal>
  );
}

export default ConfirmChangesModal;
