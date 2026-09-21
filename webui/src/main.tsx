import * as Sentry from '@sentry/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { RouterProvider, ZenProvider } from '@umami/react-zen';
import { NextIntlClientProvider } from 'next-intl';
import { BrowserRouter } from 'react-router';
import { useEffect } from 'react';
import '@umami/react-zen/styles.full.css';
import './app/global.css';
import '@fontsource/inter/300.css';
import '@fontsource/inter/400.css';
import '@fontsource/inter/500.css';
import '@fontsource/inter/600.css';
import '@fontsource/inter/700.css';
import 'chartjs-adapter-date-fns';
import { ErrorBoundary } from '@/components/common/ErrorBoundary';
import { useLocale } from '@/components/hooks/useLocale';
import Routes from './routes';

Sentry.init({
  dsn: 'https://194955540f7e4e309cdca9e3784460e9@rustrak-api.edideaur.works/41',
});

const client = new QueryClient({
  defaultOptions: {
    queries: { retry: false, refetchOnWindowFocus: false, staleTime: 1000 * 60 },
  },
});

function MessagesProvider({ children }: { children: React.ReactNode }) {
  const { locale, messages, dir } = useLocale() as any;
  useEffect(() => {
    document.documentElement.setAttribute('dir', dir ?? 'ltr');
    document.documentElement.setAttribute('lang', locale ?? 'en-US');
  }, [locale, dir]);
  return (
    <NextIntlClientProvider locale={locale ?? 'en-US'} messages={messages?.[locale ?? 'en-US'] ?? {}} onError={() => null}>
      {children}
    </NextIntlClientProvider>
  );
}

export default function App() {
  return (
    <ZenProvider>
      <BrowserRouter>
        <RouterProvider>
          <MessagesProvider>
            <QueryClientProvider client={client}>
              <ErrorBoundary>
                <Routes />
              </ErrorBoundary>
            </QueryClientProvider>
          </MessagesProvider>
        </RouterProvider>
      </BrowserRouter>
    </ZenProvider>
  );
}

import { createRoot } from 'react-dom/client';
createRoot(document.getElementById('root')!).render(<App />);
