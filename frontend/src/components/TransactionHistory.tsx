"use client";

import { useMemo, useState } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { ExplorerMenu } from "./ExplorerMenu";
import { LoadingSpinner } from "./LoadingSpinner";
import { resolveNetwork, type StellarNetwork } from "@/lib/explorerLinks";

export type TxType = "deposit" | "withdraw" | "harvest" | "all";
export type TxStatus = "confirmed" | "pending" | "failed" | "all";

export interface Transaction {
  hash: string;
  date: string;
  type: Exclude<TxType, "all">;
  amount: string;
  status: Exclude<TxStatus, "all">;
}

export interface TransactionHistoryProps {
  transactions: Transaction[];
  /** @deprecated Prefer `network`; retained for existing callers. */
  explorerBase?: string;
  onLoadMore?: () => Promise<void> | void;
  network?: StellarNetwork;
}

type SortField = "date" | "amount" | "status";
const PAGE_SIZE = 25;

function csvCell(value: string): string {
  return `"${value.replaceAll('"', '""')}"`;
}

function shortHash(hash: string): string {
  return hash.length > 14 ? `${hash.slice(0, 7)}…${hash.slice(-6)}` : hash;
}

export default function TransactionHistory({
  transactions,
  explorerBase,
  onLoadMore,
  network,
}: TransactionHistoryProps) {
  const router = useRouter();
  const pathname = usePathname();
  const searchParams = useSearchParams();
  const [search, setSearch] = useState(searchParams.get("q") ?? "");
  const [type, setType] = useState<TxType>((searchParams.get("type") as TxType) ?? "all");
  const [status, setStatus] = useState<TxStatus>((searchParams.get("status") as TxStatus) ?? "all");
  const [from, setFrom] = useState(searchParams.get("from") ?? "");
  const [to, setTo] = useState(searchParams.get("to") ?? "");
  const [minAmount, setMinAmount] = useState(searchParams.get("minAmt") ?? "");
  const [maxAmount, setMaxAmount] = useState(searchParams.get("maxAmt") ?? "");
  const [sortField, setSortField] = useState<SortField>("date");
  const [ascending, setAscending] = useState(false);
  const [page, setPage] = useState(1);
  const [visibleCount, setVisibleCount] = useState(PAGE_SIZE);
  const [loadingMore, setLoadingMore] = useState(false);

  const activeNetwork = network ?? (explorerBase?.includes("mainnet") ? "mainnet" : resolveNetwork());
  const filtered = useMemo(() => {
    const query = search.trim().toLowerCase();
    return transactions.filter((tx) => {
      const amount = Number(tx.amount);
      const timestamp = new Date(tx.date).getTime();
      if (type !== "all" && tx.type !== type) return false;
      if (status !== "all" && tx.status !== status) return false;
      if (query && !tx.hash.toLowerCase().includes(query)) return false;
      if (from && timestamp < new Date(`${from}T00:00:00Z`).getTime()) return false;
      if (to && timestamp > new Date(`${to}T23:59:59.999Z`).getTime()) return false;
      if (minAmount && amount < Number(minAmount)) return false;
      if (maxAmount && amount > Number(maxAmount)) return false;
      return true;
    });
  }, [transactions, search, type, status, from, to, minAmount, maxAmount]);

  const sorted = useMemo(() => [...filtered].sort((a, b) => {
    const result = sortField === "date"
      ? a.date.localeCompare(b.date)
      : sortField === "amount"
        ? Number(a.amount) - Number(b.amount)
        : a.status.localeCompare(b.status);
    return ascending ? result : -result;
  }), [filtered, sortField, ascending]);

  const totalPages = Math.max(1, Math.ceil(sorted.length / PAGE_SIZE));
  const pageRows = sorted.slice((Math.min(page, totalPages) - 1) * PAGE_SIZE, Math.min(page, totalPages) * PAGE_SIZE);
  const rows = onLoadMore ? sorted.slice(0, visibleCount) : pageRows;
  const hasFilters = Boolean(search || type !== "all" || status !== "all" || from || to || minAmount || maxAmount);

  function updateUrl(next: Partial<Record<string, string>>) {
    const values: Record<string, string> = {
      q: search, type, status, from, to, minAmt: minAmount, maxAmt: maxAmount,
      ...next,
    };
    const params = new URLSearchParams();
    for (const [key, value] of Object.entries(values)) {
      if (value && value !== "all") params.set(key, value);
    }
    const query = params.toString();
    router.replace(`${pathname}${query ? `?${query}` : ""}`, { scroll: false });
  }

  function setFilter(setter: (value: string) => void, key: string, value: string) {
    setter(value);
    setPage(1);
    setVisibleCount(PAGE_SIZE);
    updateUrl({ [key]: value });
  }

  function clearFilters() {
    setSearch(""); setType("all"); setStatus("all"); setFrom(""); setTo("");
    setMinAmount(""); setMaxAmount(""); setPage(1); setVisibleCount(PAGE_SIZE);
    router.replace(pathname, { scroll: false });
  }

  function exportCsv() {
    const content = [
      ["date", "type", "amount", "status", "hash"].map(csvCell).join(","),
      ...sorted.map((tx) => [tx.date, tx.type, tx.amount, tx.status, tx.hash].map(csvCell).join(",")),
    ].join("\r\n");
    const url = URL.createObjectURL(new Blob([content], { type: "text/csv;charset=utf-8" }));
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = "transactions.csv";
    anchor.click();
    window.setTimeout(() => URL.revokeObjectURL(url), 0);
  }

  async function loadMore() {
    if (loadingMore) return;
    setLoadingMore(true);
    try {
      await onLoadMore?.();
      setVisibleCount((count) => count + PAGE_SIZE);
    } finally {
      setLoadingMore(false);
    }
  }

  function header(field: SortField, label: string) {
    const active = sortField === field;
    return (
      <th scope="col" className="whitespace-nowrap px-4 py-3 text-left text-xs font-semibold uppercase tracking-wide text-zinc-500 dark:text-zinc-400">
        <button type="button" onClick={() => { setAscending(active ? !ascending : false); setSortField(field); }} aria-label={`Sort by ${label}`} aria-sort={active ? (ascending ? "ascending" : "descending") : "none"}>
          {label}{active ? (ascending ? " ↑" : " ↓") : ""}
        </button>
      </th>
    );
  }

  return (
    <section aria-label="Transaction history" className="flex flex-col gap-4 rounded-2xl bg-white p-4 shadow-sm dark:bg-zinc-900">
      <div className="flex flex-wrap items-end gap-3">
        <label className="flex flex-col gap-1 text-xs text-zinc-500 dark:text-zinc-400">Search transaction hash
          <input type="search" value={search} onChange={(e) => setFilter(setSearch, "q", e.target.value)} className="h-9 w-48 rounded-lg border border-zinc-200 px-3 text-sm dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100" />
        </label>
        <label className="flex flex-col gap-1 text-xs text-zinc-500 dark:text-zinc-400">Type
          <select value={type} onChange={(e) => setFilter(setType, "type", e.target.value)} className="h-9 rounded-lg border border-zinc-200 px-2 text-sm dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100">
            <option value="all">All</option><option value="deposit">Deposit</option><option value="withdraw">Withdraw</option><option value="harvest">Harvest</option>
          </select>
        </label>
        <label className="flex flex-col gap-1 text-xs text-zinc-500 dark:text-zinc-400">Status
          <select value={status} onChange={(e) => setFilter(setStatus, "status", e.target.value)} className="h-9 rounded-lg border border-zinc-200 px-2 text-sm dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100">
            <option value="all">All</option><option value="confirmed">Confirmed</option><option value="pending">Pending</option><option value="failed">Failed</option>
          </select>
        </label>
        <label className="flex flex-col gap-1 text-xs text-zinc-500 dark:text-zinc-400">From<input type="date" value={from} onChange={(e) => setFilter(setFrom, "from", e.target.value)} className="h-9 rounded-lg border border-zinc-200 px-2 text-sm dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100" /></label>
        <label className="flex flex-col gap-1 text-xs text-zinc-500 dark:text-zinc-400">To<input type="date" value={to} onChange={(e) => setFilter(setTo, "to", e.target.value)} className="h-9 rounded-lg border border-zinc-200 px-2 text-sm dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100" /></label>
        <label className="flex flex-col gap-1 text-xs text-zinc-500 dark:text-zinc-400">Min amount<input type="number" min="0" value={minAmount} onChange={(e) => setFilter(setMinAmount, "minAmt", e.target.value)} className="h-9 w-28 rounded-lg border border-zinc-200 px-2 text-sm dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100" /></label>
        <label className="flex flex-col gap-1 text-xs text-zinc-500 dark:text-zinc-400">Max amount<input type="number" min="0" value={maxAmount} onChange={(e) => setFilter(setMaxAmount, "maxAmt", e.target.value)} className="h-9 w-28 rounded-lg border border-zinc-200 px-2 text-sm dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100" /></label>
        {hasFilters && <button type="button" onClick={clearFilters} className="h-9 rounded-lg border border-zinc-200 px-3 text-sm text-zinc-600 hover:bg-zinc-50 dark:border-zinc-700 dark:text-zinc-300 dark:hover:bg-zinc-800">Clear filters</button>}
        <button type="button" onClick={exportCsv} className="ml-auto h-9 rounded-lg bg-zinc-900 px-4 text-sm font-medium text-white hover:bg-zinc-700 dark:bg-zinc-100 dark:text-black">Export CSV</button>
      </div>

      {hasFilters && <p className="text-xs text-zinc-500 dark:text-zinc-400" aria-live="polite">Showing {sorted.length} of {transactions.length} transactions</p>}
      <div className="overflow-x-auto rounded-xl border border-zinc-100 dark:border-zinc-800" style={{ WebkitOverflowScrolling: "touch" }}>
        <table className="min-w-full divide-y divide-zinc-100 dark:divide-zinc-800">
          <thead className="bg-zinc-50 dark:bg-zinc-800/50"><tr>
            {header("date", "Date")}<th scope="col" className="px-4 py-3 text-left text-xs font-semibold uppercase text-zinc-500">Type</th>{header("amount", "Amount")}{header("status", "Status")}<th scope="col" className="px-4 py-3 text-left text-xs font-semibold uppercase text-zinc-500">Hash</th>
          </tr></thead>
          <tbody className="divide-y divide-zinc-50 dark:divide-zinc-800">
            {rows.length === 0 ? <tr><td colSpan={5} className="py-10 text-center text-sm text-zinc-400">No transactions found.</td></tr> : rows.map((tx) => (
              <tr key={tx.hash} className="hover:bg-zinc-50 dark:hover:bg-zinc-800/40">
                <td className="whitespace-nowrap px-4 py-3 font-mono text-sm">{Number.isNaN(Date.parse(tx.date)) ? tx.date : new Date(tx.date).toISOString().replace("T", " ").slice(0, 19)}</td>
                <td className="px-4 py-3 text-sm capitalize">{tx.type}</td><td className="whitespace-nowrap px-4 py-3 font-mono text-sm">{tx.amount}</td>
                <td className="px-4 py-3 text-sm capitalize">{tx.status}</td>
                <td className="whitespace-nowrap px-4 py-3"><ExplorerMenu value={tx.hash} type="tx" network={activeNetwork} className="text-blue-600 hover:text-blue-800 dark:text-blue-400 dark:hover:text-blue-300" /><span className="sr-only">{shortHash(tx.hash)}</span></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {onLoadMore ? (
        visibleCount < sorted.length && <button type="button" onClick={() => void loadMore()} disabled={loadingMore} className="self-center rounded-lg border px-4 py-2 text-sm disabled:opacity-50">{loadingMore ? <LoadingSpinner size="sm" label="Loading more transactions…" /> : "Load more"}</button>
      ) : (
        <nav aria-label="Transaction pages" className="flex items-center justify-center gap-3 text-sm">
          <button type="button" onClick={() => setPage((value) => Math.max(1, value - 1))} disabled={page <= 1} className="rounded border px-3 py-1 disabled:opacity-40">Previous</button>
          <span aria-live="polite">Page {Math.min(page, totalPages)} of {totalPages}</span>
          <button type="button" onClick={() => setPage((value) => Math.min(totalPages, value + 1))} disabled={page >= totalPages} className="rounded border px-3 py-1 disabled:opacity-40">Next</button>
        </nav>
      )}
    </section>
  );
}
