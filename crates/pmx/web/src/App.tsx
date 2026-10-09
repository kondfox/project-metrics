import { Controls } from "./components/layout/Controls";
import { Footer } from "./components/layout/Footer";
import { Header } from "./components/layout/Header";
import { Notices } from "./components/layout/Notices";
import { Banner } from "./components/ui/Banner";
import { ActivitySection } from "./sections/ActivitySection";
import { AiSection } from "./sections/AiSection";
import { CodeHealthSection } from "./sections/CodeHealthSection";
import { SecuritySection } from "./sections/SecuritySection";
import { HeadlineSection } from "./sections/HeadlineSection";
import { MultiStackSection } from "./sections/MultiStackSection";
import { QualitySection } from "./sections/QualitySection";
import { useRangeMetrics } from "./state/hooks";

/** The project dashboard. A new area of metrics is a new section here. */
export function App() {
  const hasRange = useRangeMetrics() != null;
  return (
    <>
      <Header />
      <Notices />
      <Controls />
      {hasRange ? (
        <>
          <HeadlineSection />
          <QualitySection />
          <SecuritySection />
          <CodeHealthSection />
          <ActivitySection />
          <MultiStackSection />
          <AiSection />
        </>
      ) : (
        <Banner tone="warn">
          This range is not a whole week or month, and custom ranges need the metrics engine.
        </Banner>
      )}
      <Footer />
    </>
  );
}
