import { readFileSync, readdirSync } from "node:fs"
import path from "node:path"
import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { MatchSnippet } from "../match-snippet"

describe("MatchSnippet", () => {
  it("turns the gateway's mark tags into mark elements", () => {
    const { container } = render(<MatchSnippet snippet="so it <mark>opens</mark> the global <mark>search</mark>" />)

    expect([...container.querySelectorAll("mark")].map(node => node.textContent)).toEqual(["opens", "search"])
    expect(container.textContent).toBe("so it opens the global search")
  })

  it("renders everything that is not a mark tag as text, creating no element for it", () => {
    const { container } = render(<MatchSnippet snippet={"before <img src=x onerror=alert(1)> after"} />)

    expect(container.querySelector("img")).toBeNull()
    expect(container.textContent).toBe("before <img src=x onerror=alert(1)> after")
  })

  it("keeps marks around injected markup honest: the tag is text, the mark is an element", () => {
    const { container } = render(<MatchSnippet snippet={"<mark><script>alert(1)</script></mark>"} />)

    expect(container.querySelector("script")).toBeNull()
    expect(container.querySelector("mark")?.textContent).toBe("<script>alert(1)</script>")
  })

  it("handles a snippet with no marks at all", () => {
    const { container } = render(<MatchSnippet snippet="plain text" />)

    expect(container.querySelectorAll("mark")).toHaveLength(0)
    expect(container.textContent).toBe("plain text")
  })
})

describe("the overlay's source", () => {
  it("never reaches for dangerouslySetInnerHTML", () => {
    const overlay = path.resolve(__dirname, "..")
    const sources = [
      path.resolve(overlay, "../global-search.tsx"),
      ...readdirSync(overlay).filter(name => /\.tsx?$/.test(name)).map(name => path.join(overlay, name)),
    ]

    const offenders = sources.filter(file => readFileSync(file, "utf8").includes("dangerouslySetInnerHTML"))

    expect(offenders).toEqual([])
  })
})
