# ts-ledger

Money helpers for a shared-expenses app. Amounts are whole numbers of cents. Runs on Node 24 (TypeScript files run
directly; imports name the `.ts` file).

## parseAmount(text)

- Accepts an optional leading `-`, digits, and optionally `.` followed by one or two digits: `"12"` (1200), `"12.5"`
  (1250), `"12.05"` (1205), `"-3.10"` (-310).
- Spaces around the text are ignored.
- Throws a `RangeError` for empty text, more than two decimals, anything else, and an amount whose absolute value is
  above 1,000,000.00 (1,000,000.00 itself is allowed).

## formatAmount(cents)

- Always two decimals, thousands separated by commas: `1250` → `"12.50"`, `-5` → `"-0.05"`, `123400` → `"1,234.00"`.

## splitBill(totalCents, weights)

- Splits a total (a whole number of cents, 0 or more) between people by whole-number weights of at least 1; returns
  one share per weight, in the same order.
- The shares add up to the total exactly, and each share differs from its exact proportional value by less than one
  cent.
- Cents left over after rounding down go one each to the people with the largest fractional remainders; on a tie the
  earlier person gets it first.
- Throws a `RangeError` for an empty list of weights, a weight below 1 or not whole, and a total that is negative or
  not whole.

## Transfer

A transfer moves through `draft` → `submitted` → `approved` → `settled`, or `submitted` → `rejected`.

| Call | Allowed from | Moves to |
|---|---|---|
| `submit()` | draft | submitted |
| `approve()` | submitted | approved |
| `reject(reason)` | submitted, and only with a reason that is not blank | rejected (the reason is kept) |
| `settle()` | approved | settled |
| `cancel()` | draft or submitted | cancelled |

Any other call throws an `Error` and changes nothing on the transfer. `history` lists every state entered, in order,
starting with `draft`.
