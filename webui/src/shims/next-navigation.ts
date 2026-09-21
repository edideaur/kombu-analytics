import { useNavigate, useLocation, useSearchParams as useRouterSearchParams } from 'react-router';

export function useRouter() {
  const navigate = useNavigate();
  return {
    push: (url: string) => navigate(url),
    replace: (url: string) => navigate(url, { replace: true }),
    back: () => navigate(-1),
    forward: () => navigate(1),
    refresh: () => {},
    prefetch: () => {},
  };
}

export function usePathname(): string {
  const location = useLocation();
  return location.pathname;
}

export function useSearchParams(): URLSearchParams {
  const [params] = useRouterSearchParams();
  return params;
}

export function redirect(url: string): never {
  window.location.href = url;
  throw new Error(`Redirecting to ${url}`);
}
