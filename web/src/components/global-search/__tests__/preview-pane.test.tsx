import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { ApiError } from "@/lib/api"
import type { SearchMatchFieldWire } from "@/lib/search-api"
import { PreviewPane } from "../preview-pane"
import { recentRows, resultRows } from "../rows"
import { reason, searchResult } from "./fixtures"

const ALL_FIELDS: SearchMatchFieldWire[] = [
  "id", "title", "body", "comment", "name", "description", "prompt", "persona", "path",
  "status", "assignee", "department", "label",
]

function renderPreview(row: ReturnType<typeof resultRows>[number] | undefined, over: Partial<Parameters<typeof PreviewPane>[0]> = {}) {
  return render(
    <PreviewPane
      row={row}
      error={null}
      hint="Type to search."
      literal={false}
      onSearchLiterally={vi.fn()}
      workbench={undefined}
      {...over}
    />,
  )
}

describe("PreviewPane", () => {
  it("states why the row matched for every field the gateway can report", () => {
    for (const field of ALL_FIELDS) {
      const row = resultRows([searchResult({ kind: "todo", id: "AAA-1", reason: [reason({ field, snippet: "why-it-matched" })] })])[0]
      const { unmount } = renderPreview(row)

      expect(screen.getByTestId("search-why").textContent).toContain("why-it-matched")
      expect(screen.getByTestId("search-why-attribution").textContent?.trim()).not.toBe("")
      unmount()
    }
  })

  it("renders a comment hit as the comment snippet, attributed to a comment", () => {
    const row = resultRows([searchResult({
      kind: "todo",
      id: "AAA-1",
      reason: [reason({ field: "comment", commentId: "wic_1", snippet: "the <mark>comment</mark> body" })],
    })])[0]

    renderPreview(row)

    expect(screen.getByTestId("search-why").textContent).toContain("the comment body")
    expect(screen.getByTestId("search-why-attribution").textContent).toBe("in a comment")
  })

  it("counts the other comments a row also matched", () => {
    const row = resultRows([searchResult({
      kind: "todo",
      id: "AAA-1",
      reason: [reason({ field: "body" }), reason({ field: "comment" }), reason({ field: "comment" })],
    })])[0]

    renderPreview(row)

    expect(screen.getByTestId("search-why-attribution").textContent).toBe("in the body · also matched 2 comments")
  })

  it("renders a facet-only reason as a facet reason rather than as blank", () => {
    const row = resultRows([searchResult({
      kind: "todo",
      id: "AAA-1",
      reason: [{ field: "assignee", snippet: "a-lead" }],
    })])[0]

    renderPreview(row)

    expect(screen.getByTestId("search-why").textContent).toContain("a-lead")
    expect(screen.getByTestId("search-why-attribution").textContent).toBe("matched the assignee filter")
  })

  it("names the kind, and the id too when the row is a Todo", () => {
    const todo = resultRows([searchResult({ kind: "todo", id: "AAA-1" })])[0]
    const { unmount } = renderPreview(todo)
    expect(screen.getByTestId("search-preview").textContent).toContain("Todo · AAA-1")
    unmount()

    renderPreview(resultRows([searchResult({ kind: "note", id: "n-1" })])[0])
    expect(screen.getByTestId("search-preview").textContent).toContain("Note")
  })

  it("gives a selected recent a preview of its own", () => {
    renderPreview(recentRows([{ id: "todo-AAA-1", label: "A row", href: "/todo/AAA-1", type: "todo" }])[0])

    const preview = screen.getByTestId("search-preview")
    expect(preview.textContent).toContain("A row")
    expect(preview.textContent).toContain("You opened this from search recently.")
  })

  it("shows the gateway's rejection verbatim and offers the literal escape", () => {
    const message = '"is:nonsense" is not a Todo status — drop the token, or pass literal=true to search for it as text'

    renderPreview(undefined, { error: new ApiError(400, message) })

    expect(screen.getByTestId("search-error").textContent).toBe(message)
    expect(screen.getByTestId("search-error-literal")).toBeTruthy()
  })

  it("drops the literal escape once the search is already literal", () => {
    renderPreview(undefined, { error: new ApiError(400, "nope"), literal: true })

    expect(screen.queryByTestId("search-error-literal")).toBeNull()
  })

  it("falls back to the hint when nothing is selected", () => {
    renderPreview(undefined)

    expect(screen.getByTestId("search-preview-hint").textContent).toBe("Type to search.")
  })
})
