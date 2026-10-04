import { isThreadEmpty } from "@database/views.ts";
import { deleteThread } from "@session/mutations.ts";

/** Deletes whichever of `threadIds` are still empty, so launching and
 * closing without a message leaves nothing behind. */
export async function pruneEmptyThreads(
  threadIds: Iterable<string>,
): Promise<void> {
  await Promise.allSettled(
    [...threadIds]
      .filter((id) => isThreadEmpty(id))
      .map((id) => deleteThread(id)),
  );
}
