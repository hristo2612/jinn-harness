import { render, screen } from "@testing-library/react"
import { Plus } from "lucide-react"
import { describe, expect, it } from "vitest"
import { LargeTitleHeader } from "../large-title-header"
import { PageScaffold } from "../page-scaffold"
import { FAB_BOTTOM_WITH_TAB, FAB_BOTTOM_WITHOUT_TAB, PRIMARY_ACTION_SLOT, PrimaryAction } from "../primary-action"

describe("PrimaryAction", () => {
  it("renders a mobile FAB and a labelled desktop trailing control from one call site", () => {
    render(
      <PageScaffold
        header={<LargeTitleHeader title="Workflows" />}
        primaryAction={
          <PrimaryAction
            aria-label="New workflow"
            label="New workflow"
            icon={<Plus />}
            onClick={() => {}}
          />
        }
      >
        <p>list</p>
      </PageScaffold>,
    )

    const slots = document.querySelectorAll(`[data-slot="${PRIMARY_ACTION_SLOT}"]`)
    expect(slots.length).toBe(2)
    const fab = document.querySelector("[data-primary-action='fab']")
    const trailing = document.querySelector("[data-primary-action='trailing']")
    expect(fab).toBeTruthy()
    expect(trailing).toBeTruthy()
    expect(fab?.className).toContain("lg:hidden")
    expect(trailing?.className).toContain("hidden")
    expect(trailing?.className).toContain("lg:inline-flex")
    expect(fab?.getAttribute("aria-label")).toBe("New workflow")
    expect(trailing?.textContent).toContain("New workflow")
  })

  it("FAB bottom offset uses --tab-bar-height and no 55/56px literal", () => {
    expect(FAB_BOTTOM_WITH_TAB).toContain("--tab-bar-height")
    expect(FAB_BOTTOM_WITH_TAB).not.toMatch(/\b5[56]px\b/)
    expect(FAB_BOTTOM_WITHOUT_TAB).not.toMatch(/\b5[56]px\b/)
    expect(FAB_BOTTOM_WITHOUT_TAB).not.toContain("--tab-bar-height")

    render(
      <PageScaffold
        header={<LargeTitleHeader title="Todos" />}
        primaryAction={<PrimaryAction aria-label="New todo" label="New Todo" onClick={() => {}} />}
      >
        <p>board</p>
      </PageScaffold>,
    )
    const fab = document.querySelector("[data-primary-action='fab']") as HTMLElement
    expect(fab.className).toContain(FAB_BOTTOM_WITH_TAB)
    expect(fab.className).not.toMatch(/\b5[56]px\b/)
    expect(fab.className).toContain("size-14")
  })

  it("hideMobileTabBar drops the tab-bar token from the FAB offset", () => {
    render(
      <PageScaffold
        hideMobileTabBar
        header={<LargeTitleHeader title="Task" />}
        primaryAction={<PrimaryAction aria-label="Save" label="Save" onClick={() => {}} />}
      >
        <p>body</p>
      </PageScaffold>,
    )
    const fab = document.querySelector("[data-primary-action='fab']") as HTMLElement
    expect(fab.className).toContain(FAB_BOTTOM_WITHOUT_TAB)
    expect(fab.className).not.toContain(FAB_BOTTOM_WITH_TAB)
  })

  it("disabled FAB drops the accent fill", () => {
    render(<PrimaryAction aria-label="New" label="New" disabled onClick={() => {}} />)
    const fab = screen.getByRole("button", { name: "New" })
    expect((fab as HTMLButtonElement).disabled).toBe(true)
    expect(fab.className).toContain("--fill-tertiary")
  })
})
