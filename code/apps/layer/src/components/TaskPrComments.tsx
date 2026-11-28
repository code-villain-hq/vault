import { useCallback, useState } from "react";
import { api } from "../lib/api";
import { usePrDetailRead } from "../lib/pr-detail-read";
import { prPermalinkIdentity } from "../lib/pr-permalink";
import { PrComments } from "./PrComments";
import { PrDetailDisclosure } from "./PrDetailDisclosure";

export function TaskPrComments({
  taskId,
  checkoutId,
  number,
  identity,
  permalink,
  refresh,
  onOpen,
}: {
  taskId: string;
  checkoutId: string;
  number: number;
  identity: string;
  permalink: string | null;
  refresh: number;
  onOpen: (url: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const load = useCallback(async () => {
    const answer = await api.githubPrComments(taskId, checkoutId, number);
    if (
      answer?.checkout_id !== checkoutId ||
      answer.number !== number ||
      !permalink ||
      prPermalinkIdentity(answer.url) !== permalink
    ) {
      throw new Error("Couldn’t read comments for this pull request. Try again.");
    }
    return answer;
  }, [taskId, checkoutId, number, permalink]);
  const read = usePrDetailRead({
    enabled: open,
    identity,
    refresh,
    load,
  });
  return (
    <PrDetailDisclosure label="Comments" open={open} onToggle={() => setOpen((shown) => !shown)}>
      <PrComments read={read} onOpen={onOpen} />
    </PrDetailDisclosure>
  );
}
