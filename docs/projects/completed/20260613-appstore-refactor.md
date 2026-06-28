# Global AppStore Refactor

Replaced per-page data fetching with a shared `AppStore` (Dioxus Signals) provided by `NavLayout`. All pages read from the same store, reducing duplicate server function calls and making state consistent across the UI.
