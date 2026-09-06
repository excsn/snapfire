import type { FaqProps } from "@generated/client";

export default function FaqPage({ questions }: FaqProps) {
  return (
    <div className="page faq">
      <h1>Questions</h1>
      <dl>
        {questions.map((question) => (
          <div key={question.asks} className="qa">
            <dt>{question.asks}</dt>
            <dd>{question.answers}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}
