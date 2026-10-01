import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { text } from "../src/app/texts";
import AiReviewPage from "../src/features/ai/AiReviewPage";
import type { AiService, AiSuggestion } from "../src/services/ai-service";

const newCategorySuggestion: AiSuggestion = {
  id: "11",
  target: { type: "asset", id: "1", title: "3胎.mp4" },
  dimension: { id: "2", name: "视觉风格" },
  category: null,
  suggestedCategoryName: "纪实",
  confidence: 0.85,
  reason: "提示词符合纪实风格",
  updatedAt: "2026-07-16T10:00:00.000Z",
};

function service(): AiService {
  return {
    listProviders: vi.fn(),
    saveProvider: vi.fn(),
    deleteProvider: vi.fn(),
    testProvider: vi.fn(),
    getSendPreview: vi.fn(),
    createSuggestions: vi.fn(),
    listPending: vi.fn().mockResolvedValue({
      items: [newCategorySuggestion],
      nextCursor: null,
    }),
    resolveSuggestion: vi.fn().mockResolvedValue(undefined),
  };
}

describe("AI 待审核建议", () => {
  it("新分类建议可以直接接受，也可以拒绝", async () => {
    const aiService = service();
    render(
      <AiReviewPage
        service={aiService}
        accessMode="readWrite"
        onBack={vi.fn()}
      />,
    );

    const accept = await screen.findByRole("button", { name: text.ai.accept });
    expect(accept).toBeEnabled();
    fireEvent.click(accept);
    await waitFor(() =>
      expect(aiService.resolveSuggestion).toHaveBeenCalledWith("11", {
        kind: "accept",
      }),
    );

    fireEvent.click(screen.getByRole("button", { name: text.ai.reject }));
    await waitFor(() =>
      expect(aiService.resolveSuggestion).toHaveBeenCalledWith("11", {
        kind: "reject",
      }),
    );
  });

  it("修改只填写分类或标签名称，不再暴露分类编号", async () => {
    const aiService = service();
    render(
      <AiReviewPage
        service={aiService}
        accessMode="readWrite"
        onBack={vi.fn()}
      />,
    );

    fireEvent.click(
      await screen.findByRole("button", { name: text.ai.modify }),
    );
    expect(screen.queryByText(text.ai.categoryId)).not.toBeInTheDocument();
    expect(
      screen.queryByRole("option", { name: text.ai.mergeIntoExisting }),
    ).not.toBeInTheDocument();

    const nameInput = screen.getByLabelText(text.ai.categoryName);
    fireEvent.change(nameInput, { target: { value: "生活纪实" } });
    fireEvent.click(screen.getByRole("button", { name: text.ai.resolve }));

    await waitFor(() =>
      expect(aiService.resolveSuggestion).toHaveBeenCalledWith("11", {
        kind: "createCategory",
        name: "生活纪实",
      }),
    );
  });
});
