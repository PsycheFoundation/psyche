import { styled } from "@linaria/react";
import { useMemo, useState } from "react";
import { useInterval } from "usehooks-ts";

const Number = styled.span`
  color: var(--color-fg);
  background: var(--color-bg);
  padding: 4px;
`;

const Container = styled.span`
  padding: 4px;
`;

export function Runtime({
  start,
  end,
  pauses,
}: {
  start: Date;
  end?: Date;
  pauses?: Array<readonly ["paused" | "unpaused", Date]>;
}) {
  const [now, setNow] = useState(Date.now());
  useInterval(() => setNow(Date.now()), 1000);

  // calculate duration we were paused
  const { pauseDuration, timeOfCurrentPause } = useMemo(() => {
    if (!pauses || !pauses.length)
      return {
        pauseDuration: 0,
        isPaused: false,
        lastPauseTime: null,
      };

    let pauseDuration = 0;
    let timeOfCurrentPause = null;

    for (const [action, timestamp] of pauses) {
      if (action === "paused") {
        timeOfCurrentPause = timestamp.valueOf();
      } else if (action === "unpaused" && timeOfCurrentPause !== null) {
        pauseDuration += timestamp.valueOf() - timeOfCurrentPause;
        timeOfCurrentPause = null;
      }
    }
    return { pauseDuration, timeOfCurrentPause };
  }, [pauses, start]);

  const endTime = end ? end.valueOf() : now;
  const rawElapsed = endTime - start.valueOf();

  // if paused, the "full" duration is start <-> time of current pause
  // if not, it's start <-> now

  // then we substract the time we spent paused from that
  const elapsed =
    (timeOfCurrentPause ? timeOfCurrentPause - start.valueOf() : rawElapsed) -
    pauseDuration;

  const days = Math.floor(elapsed / (1000 * 60 * 60 * 24));
  const hours = Math.floor(
    (elapsed % (1000 * 60 * 60 * 24)) / (1000 * 60 * 60),
  );
  const minutes = Math.floor((elapsed % (1000 * 60 * 60)) / (1000 * 60));
  const seconds = Math.floor((elapsed % (1000 * 60)) / 1000);
  return (
    <Container>
      <Number>{days}d</Number>:<Number>{hours}h</Number>:
      <Number>{minutes}m</Number>:<Number>{seconds}s</Number>
    </Container>
  );
}
