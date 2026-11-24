import { Component, type ReactNode } from "react";
import { withTranslation, type WithTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { friendlyError } from "@/lib/errors";
import { log } from "@/lib/log";

interface Props extends WithTranslation {
  children: ReactNode;
}

class Boundary extends Component<Props, { error: unknown }> {
  state = { error: null as unknown };
  static getDerivedStateFromError(error: unknown) {
    return { error };
  }
  componentDidCatch(error: unknown) {
    log("error", `render error: ${error instanceof Error ? error.message : String(error)}`);
  }
  render() {
    if (!this.state.error) return this.props.children;
    const { t } = this.props;
    const f = friendlyError(this.state.error, t);
    return (
      <div className="grid h-full place-items-center p-8">
        <div className="max-w-md text-center">
          <h1 className="t-title">{f.title}</h1>
          <p className="text-muted-foreground t-body mt-2">{f.fix}</p>
          <Button className="mt-6" onClick={() => location.reload()}>
            {t("common.retry")}
          </Button>
        </div>
      </div>
    );
  }
}

export const ErrorBoundary = withTranslation()(Boundary);
