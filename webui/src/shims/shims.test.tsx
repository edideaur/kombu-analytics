import { describe, it, expect } from 'vitest';
import { render, screen, act } from '@testing-library/react';
import React from 'react';
import { MemoryRouter } from 'react-router';
import { useRouter, usePathname, useSearchParams, redirect } from './next-navigation';
import Link from './next-link';
import Script from './next-script';
import { NextIntlClientProvider } from './next-intl';

describe('WebUI Shims Coverage', () => {
  it('tests next-navigation hooks and methods', () => {
    function TestNav() {
      const router = useRouter();
      const pathname = usePathname();
      const searchParams = useSearchParams();

      return (
        <div>
          <span data-testid="path">{pathname}</span>
          <span data-testid="param">{searchParams.get('test')}</span>
          <button onClick={() => router.push('/new')}>Push</button>
          <button onClick={() => router.replace('/replace')}>Replace</button>
          <button onClick={() => router.back()}>Back</button>
          <button onClick={() => router.forward()}>Forward</button>
          <button onClick={() => router.refresh()}>Refresh</button>
          <button onClick={() => router.prefetch('/pref')}>Prefetch</button>
        </div>
      );
    }

    render(
      <MemoryRouter initialEntries={['/current?test=val']}>
        <TestNav />
      </MemoryRouter>
    );

    expect(screen.getByTestId('path').textContent).toBe('/current');
    expect(screen.getByTestId('param').textContent).toBe('val');

    act(() => {
      screen.getByText('Push').click();
      screen.getByText('Replace').click();
      screen.getByText('Back').click();
      screen.getByText('Forward').click();
      screen.getByText('Refresh').click();
      screen.getByText('Prefetch').click();
    });
  });

  it('tests redirect function', () => {
    expect(() => redirect('/auth/login')).toThrow('Redirecting to /auth/login');
  });

  it('tests Link shim branches', () => {
    render(
      <MemoryRouter>
        <Link href="/about">About</Link>
        <Link to="/contact">Contact</Link>
        <Link>Fallback</Link>
      </MemoryRouter>
    );

    expect(screen.getByText('About').getAttribute('href')).toBe('/about');
    expect(screen.getByText('Contact').getAttribute('href')).toBe('/contact');
    expect(screen.getByText('Fallback').getAttribute('href')).toBe('/');
  });

  it('tests Script shim returns null', () => {
    const { container } = render(<Script src="/test.js" />);
    expect(container.firstChild).toBeNull();
  });

  it('tests NextIntlClientProvider export', () => {
    const { container } = render(
      <NextIntlClientProvider locale="en-US" messages={{}}>
        <div>Hello Intl</div>
      </NextIntlClientProvider>
    );
    expect(container.textContent).toBe('Hello Intl');
  });
});
