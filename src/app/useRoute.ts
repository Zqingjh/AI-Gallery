import { useCallback, useEffect, useState } from "react";

export type AppRoute = "/" | "/settings" | "/ai-review" | "/efficiency";

function readRoute(): AppRoute {
  switch (window.location.pathname) {
    case "/settings":
      return "/settings";
    case "/ai-review":
      return "/ai-review";
    case "/efficiency":
      return "/efficiency";
    default:
      return "/";
  }
}

export function useRoute() {
  const [route, setRoute] = useState<AppRoute>(readRoute);

  useEffect(() => {
    const handlePopState = () => setRoute(readRoute());
    window.addEventListener("popstate", handlePopState);
    return () => window.removeEventListener("popstate", handlePopState);
  }, []);

  const navigate = useCallback((nextRoute: AppRoute) => {
    window.history.pushState(null, "", nextRoute);
    setRoute(nextRoute);
  }, []);

  return { route, navigate } as const;
}
