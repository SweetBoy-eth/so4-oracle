'use client';
import { useParams } from 'next/navigation';
import { useStreamBalance } from '@/hooks/useStreamBalance';
import styles from './stream.module.css';

// Mock stream data
const MOCK: Record<string, any> = {
  '0': {
    id: '0', sender: 'GABC123456789012345678901234567890123456789012345678WXYZ',
    recipient: 'GBOB123456789012345678901234567890123456789012345678WXYZ',
    token: 'XLM', ratePerSecond: 116n,
    startTime: 1735689600, stopTime: 1738368000,
    withdrawn: 1_000_000n, cancelled: false,
  },
};

function fmt(stroops: bigint): string {
  return (Number(stroops) / 1e7).toFixed(7);
}

export default function StreamDetail() {
  const { id }   = useParams<{ id: string }>();
  const stream   = MOCK[id];
  const balance  = useStreamBalance(
    stream?.ratePerSecond ?? 0n,
    stream?.withdrawn     ?? 0n,
    stream?.startTime     ?? 0,
    stream?.stopTime      ?? 0,
  );

  if (!stream) {
    return <div className={styles.notFound}>Stream not found.</div>;
  }

  return (
    <div className={styles.page}>
      <h1 className={styles.title}>Stream #{id}</h1>

      <div className={styles.balanceCard}>
        <p className={styles.balanceLabel}>Withdrawable Balance</p>
        <p className={styles.balanceValue}>{fmt(balance)} XLM</p>
        <p className={styles.balanceRate}>
          {(Number(stream.ratePerSecond) * 86400 / 1e7).toFixed(4)} XLM/day
        </p>
      </div>

      <div className={styles.details}>
        {[
          ['From',  `${stream.sender.slice(0,5)}...${stream.sender.slice(-4)}`],
          ['To',    `${stream.recipient.slice(0,5)}...${stream.recipient.slice(-4)}`],
          ['Token', stream.token],
          ['Status', stream.cancelled ? 'Cancelled' : 'Active'],
        ].map(([k, v]) => (
          <div key={k} className={styles.detailRow}>
            <span className={styles.detailKey}>{k}</span>
            <span className={styles.detailVal}>{v}</span>
          </div>
        ))}
      </div>

      {!stream.cancelled && (
        <div className={styles.actions}>
          <button className={styles.btnWithdraw}>Withdraw</button>
          <button className={styles.btnCancel}>Cancel Stream</button>
        </div>
      )}
    </div>
  );
}
