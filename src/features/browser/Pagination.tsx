import { useState } from "react";
import { PAGE_SIZES, pageWindow } from "./browserState";

interface PaginationProps {
  /** Zero-based page index. */
  page: number;
  pageCount: number;
  pageSize: number;
  onPage: (page: number) => void;
  onPageSize: (pageSize: number) => void;
}

export function Pagination({ page, pageCount, pageSize, onPage, onPageSize }: PaginationProps) {
  const [goTo, setGoTo] = useState("");
  const last = Math.max(pageCount - 1, 0);

  function submitGoTo() {
    const target = Number.parseInt(goTo, 10);
    if (Number.isFinite(target) && target >= 1 && target <= pageCount) {
      onPage(target - 1);
      setGoTo("");
    }
  }

  return (
    <nav className="pagination" aria-label="Pagination">
      <div className="pagination__pages">
        <button
          type="button"
          className="button button--secondary button--small"
          disabled={page <= 0}
          onClick={() => {
            onPage(page - 1);
          }}
        >
          Previous
        </button>
        {pageWindow(page, pageCount).map((value, index) =>
          value === null ? (
            <span key={`gap-${String(index)}`} className="pagination__gap" aria-hidden="true">
              …
            </span>
          ) : (
            <button
              key={value}
              type="button"
              className={
                value === page
                  ? "button button--small pagination__page"
                  : "button button--secondary button--small pagination__page"
              }
              aria-label={`Page ${String(value + 1)}`}
              aria-current={value === page ? "page" : undefined}
              onClick={() => {
                onPage(value);
              }}
            >
              {value + 1}
            </button>
          ),
        )}
        <button
          type="button"
          className="button button--secondary button--small"
          disabled={page >= last}
          onClick={() => {
            onPage(page + 1);
          }}
        >
          Next
        </button>
      </div>
      <div className="pagination__options">
        <span className="pagination__status">
          Page {page + 1} of {Math.max(pageCount, 1)}
        </span>
        <label className="pagination__option">
          <span>Per page</span>
          <select
            className="input"
            value={pageSize}
            onChange={(event) => {
              onPageSize(Number(event.target.value));
            }}
          >
            {PAGE_SIZES.map((size) => (
              <option key={size} value={size}>
                {size}
              </option>
            ))}
          </select>
        </label>
        <form
          className="pagination__option"
          onSubmit={(event) => {
            event.preventDefault();
            submitGoTo();
          }}
        >
          <label htmlFor="pagination-go-to">Go to</label>
          <input
            id="pagination-go-to"
            className="input pagination__go-to"
            type="number"
            min={1}
            max={Math.max(pageCount, 1)}
            value={goTo}
            onChange={(event) => {
              setGoTo(event.target.value);
            }}
          />
          <button type="submit" className="button button--secondary button--small">
            Go
          </button>
        </form>
      </div>
    </nav>
  );
}
