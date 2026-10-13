import { Footer } from "./components/layout/Footer";
import { Header } from "./components/layout/Header";
import { Notices } from "./components/layout/Notices";
import { StickyTop } from "./components/layout/StickyTop";
import { Banner } from "./components/ui/Banner";
import { AiSection } from "./sections/AiSection";
import { CodeHealthSection } from "./sections/CodeHealthSection";
import { SecuritySection } from "./sections/SecuritySection";
import { HeadlineSection } from "./sections/HeadlineSection";
import { MultiStackSection } from "./sections/MultiStackSection";
import { QualitySection } from "./sections/QualitySection";
import { ReviewSection } from "./sections/ReviewSection";
import { VelocitySection } from "./sections/VelocitySection";
import { useRangeMetrics } from "./state/hooks";

/** The project dashboard, in the fleet's column order (fleet/headlines.ts). A new area of metrics is a new section here. */
export function App() {
  const hasRange = useRangeMetrics() != null;
  return (
    <>
      <StickyTop>
        <Header />
      </StickyTop>
      <Notices />
      {hasRange ? (
        <>
          <HeadlineSection />
          <VelocitySection />
          <QualitySection />
          <SecuritySection />
          <MultiStackSection />
          <AiSection />
          <ReviewSection />
          <CodeHealthSection />
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
