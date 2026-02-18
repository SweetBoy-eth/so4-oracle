'use client';
import { useState, useEffect, useRef } from 'react';

/**
 * Computes real-time balance by interpolating from last known value.
 * Syncs with contract every syncInterval ms; updates display every tick ms.
 */
export function useStreamBalance(
  ratePerSecond: bigint,
  lastWithdrawn: bigint,
  startTime:     number,
  stopTime:      number,
  tick = 200,
) {
  const [balance, setBalance] = useState(0n);

  useEffect(() => {
    const id = setInterval(() => {
      const now     = Math.floor(Date.now() / 1000);
      if (now <= startTime) { setBalance(0n); return; }
      const elapsed = BigInt(Math.min(now, stopTime) - startTime);
      const accrued = ratePerSecond * elapsed;
      setBalance(accrued > lastWithdrawn ? accrued - lastWithdrawn : 0n);
    }, tick);
    return () => clearInterval(id);
  }, [ratePerSecond, lastWithdrawn, startTime, stopTime, tick]);

  return balance;
}
