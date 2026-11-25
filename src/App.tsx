import { lazy, Suspense, useEffect } from "react";
import { initChatListeners } from "@/features/chat/chat-store";
import { createHashRouter, Navigate, Outlet, RouterProvider, useLocation } from "react-router";
import { Toaster } from "@/components/ds/toaster";
import { ErrorBoundary } from "@/components/layout/error-boundary";
import { Spinner } from "@/components/ds/spinner";
import { useSettings } from "@/hooks";
import { HomePage } from "@/features/home/home-page";
import { SettingsPage } from "@/features/settings/settings-page";
import { SetupPage } from "@/features/setup/setup-page";
import { TooltipProvider } from "@/components/ui/tooltip";

const ProjectPage = lazy(() => import("@/features/project/project-page"));
const DesignPage = lazy(() => import("@/features/settings/design-page"));

/** Eerste start: stuur door naar de wizard tot de installatie is afgerond of overgeslagen. */
function Root() {
  const { data, isPending } = useSettings();
  const { pathname } = useLocation();
  if (isPending) {
    return (
      <div className="grid h-full place-items-center">
        <Spinner className="size-6" />
      </div>
    );
  }
  if (!["true", "skipped"].includes(data?.setup_completed ?? "") && pathname !== "/setup")
    return <Navigate to="/setup" replace />;
  return (
    <Suspense fallback={null}>
      <Outlet />
    </Suspense>
  );
}

const router = createHashRouter([
  {
    element: <Root />,
    children: [
      { path: "/", element: <HomePage /> },
      { path: "/setup", element: <SetupPage /> },
      { path: "/settings", element: <SettingsPage /> },
      { path: "/project/:id", element: <ProjectPage /> },
      { path: "/design", element: <DesignPage /> },
      { path: "*", element: <Navigate to="/" replace /> },
    ],
  },
]);

export function App() {
  useEffect(() => initChatListeners(), []);
  return (
    <ErrorBoundary>
      <TooltipProvider delayDuration={300}>
        <RouterProvider router={router} />
        <Toaster />
      </TooltipProvider>
    </ErrorBoundary>
  );
}
