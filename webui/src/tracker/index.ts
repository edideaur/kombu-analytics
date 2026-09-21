/** Public types for the browser tracker. */
export type TrackedProperties = {
  /**
   * Hostname of server
   *
   * @description extracted from `window.location.hostname`
   * @example 'analytics.umami.is'
   */
  hostname?: string;

  /** Distinct ID associated with the current visitor. */
  id?: string;

  /**
   * Browser language
   *
   * @description extracted from `window.navigator.language`
   * @example 'en-US', 'fr-FR'
   */
  language?: string;

  /**
   * Page referrer
   *
   * @description extracted from `document.referrer`
   * @example 'https://analytics.umami.is/docs/getting-started'
   */
  referrer?: string;

  /**
   * Screen dimensions
   *
   * @description extracted from `window.screen.width` and `window.screen.height`
   * @example '1920x1080', '2560x1440'
   */
  screen?: string;

  /** Tag configured on the tracker script. */
  tag?: string;

  /**
   * Page title
   *
   * @description extracted from `document.querySelector('head > title')`
   * @example 'umami'
   */
  title?: string;

  /**
   * Page url
   *
   * @description normalized from `window.location.href`
   * @example 'https://analytics.umami.is/docs/getting-started'
   */
  url?: string;

  /**
   * Website ID (required)
   *
   * @example 'b59e9c65-ae32-47f1-8400-119fcf4861c4'
   */
  website: string;
};

export type WithRequired<T, K extends keyof T> = T & { [P in K]-?: T[P] };

export type EventDataValue = boolean | number | string | null | EventData | EventDataValue[];

/**
 *
 * Event Data can work with any JSON data. There are a few rules in place to maintain performance.
 * - Numbers have a max precision of 4.
 * - Strings have a max length of 500.
 * - Arrays are converted to a String, with the same max length of 500.
 * - Objects have a max of 50 properties. Arrays are considered 1 property.
 */
export interface EventData {
  [key: string]: EventDataValue;
}

export type EventProperties = {
  /**
   * NOTE: event names will be truncated past 50 characters
   */
  name: string;
  data?: EventData;
} & WithRequired<TrackedProperties, 'website'>;
export type PageViewProperties = WithRequired<TrackedProperties, 'website'>;
export type CustomEventFunction = (
  props: PageViewProperties,
) => EventProperties | PageViewProperties;

export type UmamiTracker = {
  track: {
    /**
     * Track a page view
     *
     * @example ```
     * umami.track();
     * ```
     */
    (): Promise<void>;

    /**
     * Track an event with a given name
     *
     * NOTE: event names will be truncated past 50 characters
     *
     * @example ```
     * umami.track('signup-button');
     * ```
     */
    (eventName: string): Promise<void>;

    /**
     * Tracks an event with dynamic data.
     *
     * NOTE: event names will be truncated past 50 characters
     *
     * When tracking events, the default properties are included in the payload. This is equivalent to running:
     *
     * ```js
     * umami.track(props => ({
     *   ...props,
     *   name: 'signup-button',
     *   data: {
     *     name: 'newsletter',
     *     id: 123
     *   }
     * }));
     * ```
     *
     * @example ```
     * umami.track('signup-button', { name: 'newsletter', id: 123 });
     * ```
     */
    (eventName: string, obj: EventData): Promise<void>;

    /**
     * Tracks a page view with custom properties
     *
     * @example ```
     * umami.track({ website: 'e676c9b4-11e4-4ef1-a4d7-87001773e9f2', url: '/home', title: 'Home page' });
     * ```
     */
    (properties: PageViewProperties): Promise<void>;

    /**
     * Tracks an event with fully customizable dynamic data
     * If you don't specify any `name` and/or `data`, it will be treated as a page view
     *
     * @example ```
     * umami.track((props) => ({ ...props, url: path }));
     * ```
     */
    (eventFunction: CustomEventFunction): Promise<void>;
  };
  identify: {
    /**
     * Identify a visitor with optional associated data.
     *
     * @example ```
     * umami.identify('user-123', { plan: 'pro' });
     * ```
     */
    (id: string, data?: EventData): Promise<void>;

    /**
     * Associate data with the current visitor. An `id` string sets the Distinct ID.
     *
     * @example ```
     * umami.identify({ id: 'user-123', plan: 'pro' });
     * ```
     */
    (data: EventData & { id?: string }): Promise<void>;
  };
  revenue: {
    (amount: number, currency?: string, data?: EventData): Promise<void>;
    (data: { amount: number; currency?: string; [key: string]: unknown }): Promise<void>;
  };
  getSession: () => {
    cache: string | undefined;
    website: string | null;
  };
};

declare global {
  interface Window {
    umami: UmamiTracker;
  }
}

type Payload = Record<string, unknown>;
type BeforeSend = (
  type: string,
  payload: Payload,
) => Payload | null | undefined | Promise<Payload | null | undefined>;
type TrackerWindow = Window &
  typeof globalThis & {
    doNotTrack?: string | number | null;
    navigator: Navigator & {
      msDoNotTrack?: string | number | null;
    };
  };
type TrackerDocument = Document & {
  currentScript: HTMLScriptElement | null;
};
type MetricEntry = PerformanceEntry & {
  activationStart: number;
  duration: number;
  hadRecentInput: boolean;
  interactionId: number;
  responseStart: number;
  startTime: number;
  value: number;
};
(window => {
  const {
    screen: { width, height },
    navigator: { language, doNotTrack: ndnt, msDoNotTrack: msdnt },
    location,
    document,
    history,
    top,
    doNotTrack,
  } = window;
  const { currentScript, referrer } = document as TrackerDocument;
  if (!currentScript) return;

  const { hostname, href, origin } = location;

  let localStorage: Storage | undefined;
  try {
    localStorage = href.startsWith('data:') ? undefined : window.localStorage;
  } catch {
    /* (DOMException) SecurityError: Access is denied for this document. */
  }

  const _data = 'data-';
  const _false = 'false';
  const _true = 'true';
  const attr = currentScript.getAttribute.bind(currentScript);
  const config = (value: string) => attr(`${_data}${value}`);

  const website = config('website-id');
  const hostUrl = config('host-url');
  const beforeSend = config('before-send');
  const tag = config('tag') || undefined;
  const autoTrack = config('auto-track') !== _false;
  const dnt = config('do-not-track') === _true;
  const excludeSearch = config('exclude-search') === _true;
  const excludeHash = config('exclude-hash') === _true;
  const domain = config('domains') || '';
  const credentials = (config('fetch-credentials') || 'omit') as RequestCredentials;
  const perf = config('performance') === _true;
  const autoPageview = config('auto-pageview') !== _false;
  const autoScroll = config('auto-scroll') !== _false;

  const domains = domain.split(',').map(n => n.trim());
  const host =
    hostUrl || currentScript.src.split('/').slice(0, -1).join('/');
  const endpoint = `${host.replace(/\/$/, '')}/api/send`;
  const screen = `${width}x${height}`;
  const eventRegex = /data-umami-event-([\w-_]+)/;
  const eventNameAttribute = `${_data}umami-event`;
  const delayDuration = 300;

  /* Helper functions */

  const normalize = (raw: string | URL): string => {
    if (!raw) return raw as string;
    try {
      const u = new URL(raw, location.href);
      if (excludeSearch) u.search = '';
      if (excludeHash) u.hash = '';
      return u.toString();
    } catch {
      return raw as string;
    }
  };

  // Strip the origin from same-origin referrers so the referrer domain
  // is never saved when it matches the current hostname
  const stripOrigin = (url: string): string =>
    url === origin || url?.startsWith(origin + '/') ? url.slice(origin.length) : url;

  const getPayload = () => ({
    website,
    screen,
    language,
    title: document.title,
    hostname,
    url: currentUrl,
    referrer: stripOrigin(currentRef),
    tag,
    id: identity ? identity : undefined,
  });

  const hasDoNotTrack = () => {
    const dnt = doNotTrack || ndnt || msdnt;
    return dnt === 1 || dnt === '1' || dnt === 'yes';
  };

  /* Event handlers */

  const handlePush = (_state: unknown, _title: string, url?: string | URL | null) => {
    if (!url) return;

    if (typeof flushPerformance === 'function') {
      flushPerformance();
    }

    if (typeof flushEngagement === 'function') {
      flushEngagement(false);
      resetEngagement?.();
    }

    currentRef = currentUrl;
    currentUrl = normalize(url);

    if (currentUrl !== currentRef && autoPageview) {
      setTimeout(track, delayDuration);
    }
  };

  const handlePathChanges = () => {
    const hook = (
      _this: History,
      method: 'pushState' | 'replaceState',
      callback: typeof handlePush,
    ) => {
      const orig = _this[method];
      return (...args: Parameters<History['pushState']>) => {
        const result = orig.apply(_this, args);
        callback.apply(null, args);
        return result;
      };
    };

    history.pushState = hook(history, 'pushState', handlePush);
    history.replaceState = hook(history, 'replaceState', handlePush);
  };

  const handleClicks = () => {
    const downloadRegex = /\.(pdf|zip|gz|tar|tgz|bz2|csv|tsv|docx?|xlsx?|pptx?|mp3|mp4|avi|mov|dmg|pkg|exe|apk|iso|epub)$/i;

    const handleAutoClick = (anchor: HTMLAnchorElement) => {
      const href = anchor.href;
      if (!href) return;
      try {
        const url = new URL(href, location.href);
        const protocol = url.protocol;
        if (protocol !== 'http:' && protocol !== 'https:') return;

        // Check file download
        if (downloadRegex.test(url.pathname)) {
          const cleanUrl = `${url.origin}${url.pathname}`;
          track('File Download', { url: cleanUrl });
          return;
        }

        // Check outbound link
        if (url.hostname && url.hostname !== hostname) {
          track('Outbound Link: Click', { url: href });
        }
      } catch {
        /* no-op */
      }
    };

    const trackElement = async (el: Element) => {
      const eventName = el.getAttribute(eventNameAttribute);
      if (eventName) {
        const eventData: EventData = {};

        el.getAttributeNames().forEach(name => {
          const match = name.match(eventRegex);
          if (match) {
            const val = el.getAttribute(name) as string;
            const key = match[1];
            if (key === 'amount' || key === 'revenue') {
              const parsed = parseFloat(val);
              eventData[key] = isNaN(parsed) ? val : parsed;
            } else {
              eventData[key] = val;
            }
          }
        });

        return track(eventName, eventData);
      }
    };
    const onClick = (e: MouseEvent) => {
      const el = e.target as Element;

      // Autocapture tagged revenue clicks: data-umami-event-amount="19.99" or data-amount="19.99"
      const revenueEl = el.closest?.('[data-umami-event-amount],[data-umami-amount],[data-amount]');
      if (revenueEl) {
        const amountStr =
          revenueEl.getAttribute('data-umami-event-amount') ||
          revenueEl.getAttribute('data-umami-amount') ||
          revenueEl.getAttribute('data-amount');
        const currencyStr =
          revenueEl.getAttribute('data-umami-event-currency') ||
          revenueEl.getAttribute('data-umami-currency') ||
          revenueEl.getAttribute('data-currency') ||
          'USD';
        if (amountStr) {
          const parsed = parseFloat(amountStr);
          if (!isNaN(parsed) && parsed > 0) {
            const evName = revenueEl.getAttribute(eventNameAttribute) || 'Purchase';
            track(evName, { amount: parsed, currency: currencyStr.toUpperCase() });
          }
        }
      }

      const eventEl = el.closest(`[${eventNameAttribute}]`);
      if (!eventEl) {
        const anchor = el.closest('a');
        if (anchor && (anchor as HTMLAnchorElement).href) {
          handleAutoClick(anchor as HTMLAnchorElement);
        }
        return;
      }

      if (eventEl.tagName === 'A' && (eventEl as HTMLAnchorElement).href) {
        const { href, target } = eventEl as HTMLAnchorElement;
        const external =
          target === '_blank' ||
          e.ctrlKey ||
          e.shiftKey ||
          e.metaKey ||
          (e.button && e.button === 1);
        if (!external) e.preventDefault();
        return trackElement(eventEl).finally(() => {
          if (!external) {
            (target === '_top' ? (top as WindowProxy).location : location).href = href;
          }
        });
      }

      return trackElement(eventEl);
    };
    document.addEventListener('click', onClick, true);
  };

  /* Tracking functions */

  const trackingDisabled = () =>
    disabled ||
    !website ||
    localStorage?.getItem('umami.disabled') ||
    (domain && !domains.includes(hostname)) ||
    (dnt && hasDoNotTrack());

  const send = async (
    payload: Payload | null | undefined,
    type = 'event',
    useBeacon = false,
  ): Promise<void> => {
    if (trackingDisabled()) return;

    const callback = (window as unknown as Record<string, unknown>)[beforeSend as string] as
      | BeforeSend
      | undefined;

    if (typeof callback === 'function') {
      payload = await Promise.resolve(callback(type, payload as Payload));
    }

    if (!payload) return;

    const bodyString = JSON.stringify({ type, payload });

    // Use navigator.sendBeacon when flushing on pagehide / unload / visibility change
    if (useBeacon && typeof navigator.sendBeacon === 'function') {
      const blob = new Blob([bodyString], { type: 'application/json' });
      navigator.sendBeacon(endpoint, blob);
      return;
    }

    try {
      const res = await fetch(endpoint, {
        keepalive: true,
        method: 'POST',
        body: bodyString,
        headers: {
          'Content-Type': 'application/json',
          'x-umami-website-id': website as string,
          'x-umami-hostname': hostname,
          ...(typeof cache !== 'undefined' && { 'x-umami-cache': cache }),
        },
        credentials,
      });

      const data = (await res.json()) as { cache?: string; disabled?: boolean } | null;
      if (data) {
        disabled = !!data.disabled;
        cache = data.cache;
      }
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
    } catch (_e) {
      /* no-op */
    }
  };

  /* Form Abandonment & Input Engagement Tracking */
  const initFormTracking = () => {
    const trackedForms = new WeakMap<HTMLFormElement, { interactedFields: Set<string>; submitted: boolean }>();

    const getFieldIdentifier = (field: HTMLElement): string => {
      return (
        field.getAttribute('name') ||
        field.getAttribute('id') ||
        field.getAttribute('placeholder') ||
        field.tagName.toLowerCase()
      ).slice(0, 50);
    };

    document.addEventListener(
      'focusin',
      (e: FocusEvent) => {
        const target = e.target as HTMLElement | null;
        if (!target) return;
        const form = target.closest('form');
        if (!form) return;

        const tag = target.tagName;
        if (tag !== 'INPUT' && tag !== 'TEXTAREA' && tag !== 'SELECT') return;
        const inputType = target.getAttribute('type') || 'text';
        if (inputType === 'hidden' || inputType === 'submit' || inputType === 'button') return;

        let record = trackedForms.get(form);
        const fieldName = getFieldIdentifier(target);
        if (!record) {
          record = { interactedFields: new Set([fieldName]), submitted: false };
          trackedForms.set(form, record);
          const formName = form.getAttribute('name') || form.getAttribute('id') || 'unnamed-form';
          track('Form Start', { form: formName, firstField: fieldName });
        } else {
          record.interactedFields.add(fieldName);
        }
      },
      true,
    );

    document.addEventListener(
      'submit',
      (e: SubmitEvent) => {
        const form = e.target as HTMLFormElement | null;
        if (!form) return;
        let record = trackedForms.get(form);
        if (!record) {
          record = { interactedFields: new Set(), submitted: true };
          trackedForms.set(form, record);
        } else {
          record.submitted = true;
        }
        const formName = form.getAttribute('name') || form.getAttribute('id') || 'unnamed-form';
        track('Form Submit', { form: formName, fieldsCount: record.interactedFields.size });
      },
      true,
    );

    // Track abandonment on unload/pagehide if form started but not submitted
    const checkAbandonment = (isBeacon: boolean) => {
      document.querySelectorAll('form').forEach(form => {
        const record = trackedForms.get(form);
        if (record && !record.submitted && record.interactedFields.size > 0) {
          const formName = form.getAttribute('name') || form.getAttribute('id') || 'unnamed-form';
          const fields = Array.from(record.interactedFields);
          const lastField = fields[fields.length - 1];
          send(
            {
              ...getPayload(),
              name: 'Form Abandon',
              data: {
                form: formName,
                lastField,
                interactedCount: fields.length,
              },
            },
            'event',
            isBeacon,
          );
          // Mark as handled so not sent twice
          record.submitted = true;
        }
      });
    };

    window.addEventListener('pagehide', () => checkAbandonment(true));
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'hidden') checkAbandonment(true);
    });
  };

  /* Scroll Depth & Reader Engagement Tracking */
  const initScrollAndEngagementTracking = () => {
    if (!autoScroll) return;

    let maxScrollDepth = 0;
    let activeEngagedStart =
      document.visibilityState === 'visible' && typeof document.hasFocus === 'function' && document.hasFocus()
        ? Date.now()
        : 0;
    let totalActiveEngagedMs = 0;
    let scrollEngagedSent = false;
    const milestonesReached = new Set<number>();

    const getScrollDepth = (): number => {
      try {
        const docEl = document.documentElement || {};
        const body = document.body || {};
        const scrollTop =
          window.pageYOffset ||
          (docEl as HTMLElement).scrollTop ||
          (body as HTMLElement).scrollTop ||
          0;
        const scrollHeight = Math.max(
          (docEl as HTMLElement).scrollHeight || 0,
          (body as HTMLElement).scrollHeight || 0,
          (docEl as HTMLElement).offsetHeight || 0,
          (body as HTMLElement).offsetHeight || 0,
          (docEl as HTMLElement).clientHeight || 0,
        );
        const clientHeight =
          window.innerHeight || (docEl as HTMLElement).clientHeight || 0;
        if (scrollHeight <= clientHeight || scrollHeight === 0) return 100;
        return Math.min(
          100,
          Math.round(((scrollTop + clientHeight) / scrollHeight) * 100),
        );
      } catch {
        return 0;
      }
    };

    const updateActiveEngagement = () => {
      const isVisible =
        document.visibilityState === 'visible' &&
        (typeof document.hasFocus !== 'function' || document.hasFocus());
      if (isVisible) {
        if (activeEngagedStart === 0) {
          activeEngagedStart = Date.now();
        }
      } else if (activeEngagedStart > 0) {
        totalActiveEngagedMs += Date.now() - activeEngagedStart;
        activeEngagedStart = 0;
      }
    };

    const getActiveEngagedSeconds = (): number => {
      let ms = totalActiveEngagedMs;
      if (activeEngagedStart > 0) {
        ms += Date.now() - activeEngagedStart;
      }
      return Math.round(ms / 1000);
    };

    flushEngagement = (isBeacon = false) => {
      if (!autoScroll || scrollEngagedSent) return;
      updateActiveEngagement();
      const engagedSec = getActiveEngagedSeconds();
      const depth = Math.max(maxScrollDepth, getScrollDepth());

      if (engagedSec >= 2 || depth >= 10) {
        scrollEngagedSent = true;
        send(
          {
            ...getPayload(),
            name: 'scroll',
            data: {
              depth,
              engaged_time: engagedSec,
              milestones: Array.from(milestonesReached)
                .sort((a, b) => a - b)
                .join(','),
            },
          },
          'event',
          isBeacon,
        );
      }
    };

    resetEngagement = () => {
      maxScrollDepth = 0;
      activeEngagedStart =
        document.visibilityState === 'visible' &&
        (typeof document.hasFocus !== 'function' || document.hasFocus())
          ? Date.now()
          : 0;
      totalActiveEngagedMs = 0;
      scrollEngagedSent = false;
      milestonesReached.clear();
    };

    const onScroll = () => {
      const current = getScrollDepth();
      if (current > maxScrollDepth) {
        maxScrollDepth = current;
        for (const m of [25, 50, 75, 100]) {
          if (current >= m) {
            milestonesReached.add(m);
          }
        }
      }
    };

    window.addEventListener('scroll', onScroll, { passive: true });
    document.addEventListener('visibilitychange', () => {
      updateActiveEngagement();
      if (document.visibilityState === 'hidden') {
        flushEngagement?.(true);
      }
    });
    window.addEventListener('blur', updateActiveEngagement);
    window.addEventListener('focus', updateActiveEngagement);
    window.addEventListener('pagehide', () => flushEngagement?.(true));
  };

  const initErrors = () => {
    window.addEventListener('error', (e: ErrorEvent) => {
      const message = (e.message || 'Unknown error').slice(0, 500);
      send(
        {
          ...getPayload(),
          message,
          stack: (e.error?.stack || '').slice(0, 500),
          data: { filename: e.filename, lineno: e.lineno, colno: e.colno },
        },
        'error',
      );
    });
    window.addEventListener('unhandledrejection', (e: PromiseRejectionEvent) => {
      const reason = e.reason;
      const message = String(reason?.message || reason || 'Unhandled promise rejection').slice(0, 500);
      send(
        {
          ...getPayload(),
          message,
          stack: (reason?.stack || '').slice(0, 500),
          data: { type: 'unhandledrejection' },
        },
        'error',
      );
    });
  };

  const init = () => {
    if (!initialized) {
      initialized = true;
      if (autoPageview) track();
      handlePathChanges();
      handleClicks();
      if (perf) initPerformance();
      initErrors();
      initFormTracking();
      initScrollAndEngagementTracking();
    }
  };

  const track = (
    name?: string | Payload | ((payload: Payload) => Payload),
    data?: EventData,
  ): Promise<void> => {
    if (typeof name === 'string') return send({ ...getPayload(), name, data });
    if (typeof name === 'object') return send({ ...name });
    if (typeof name === 'function') return send(name(getPayload()));
    return send(getPayload());
  };

  const identify = (
    id: string | (EventData & { id?: string }),
    data?: EventData,
  ): Promise<void> => {
    const nextIdentity = typeof id === 'string' ? id : id.id;

    if (nextIdentity !== undefined) {
      identity = nextIdentity;
    }

    cache = '';
    return send(
      {
        ...getPayload(),
        data: typeof id === 'object' ? id : data,
      },
      'identify',
    );
  };

  /* Performance */

  const initPerformance = () => {
    const metrics: Record<string, number> = {};
    let sent = false;
    let timeoutId: ReturnType<typeof setTimeout> | undefined;
    let isInitialLoad = true;
    let activationStart = 0;
    let pageStartTime = 0;

    const observe = (type: string, callback: (entry: MetricEntry) => void) => {
      try {
        const observer = new PerformanceObserver(list => {
          (list.getEntries() as MetricEntry[]).forEach(callback);
        });
        observer.observe({ type, buffered: true });
      } catch {
        /* not supported */
      }
    };

    // TTFB
    observe('navigation', entry => {
      activationStart = entry.activationStart || 0;
      metrics.ttfb = Math.max(entry.responseStart - activationStart, 0);
    });

    // FCP
    observe('paint', entry => {
      if (entry.name === 'first-contentful-paint') {
        metrics.fcp = Math.max(entry.startTime - activationStart, 0);
      }
    });

    // LCP
    observe('largest-contentful-paint', entry => {
      metrics.lcp = Math.max(entry.startTime - activationStart, 0);
    });

    // CLS - session windows algorithm (gap < 1s, max 5s duration; report worst window)
    let clsSessionValue = 0;
    let clsSessionEntries: MetricEntry[] = [];
    observe('layout-shift', entry => {
      if (!entry.hadRecentInput) {
        const lastEntry = clsSessionEntries[clsSessionEntries.length - 1];
        const firstEntry = clsSessionEntries[0];
        if (
          lastEntry &&
          entry.startTime - lastEntry.startTime - lastEntry.duration < 1000 &&
          entry.startTime - firstEntry.startTime < 5000
        ) {
          clsSessionValue += entry.value;
          clsSessionEntries.push(entry);
        } else {
          clsSessionValue = entry.value;
          clsSessionEntries = [entry];
        }
        if (clsSessionValue > (metrics.cls || 0)) {
          metrics.cls = clsSessionValue;
        }
      }
    });

    // INP - group by interactionId, 98th percentile, 40ms threshold
    let interactions: Record<number, number> = {};
    let inpObserver: PerformanceObserver | undefined;
    const recordInteractions = (entries: PerformanceEntryList) => {
      (entries as MetricEntry[]).forEach(entry => {
        if (entry.interactionId) {
          const existing = interactions[entry.interactionId];
          if (!existing || entry.duration > existing) {
            interactions[entry.interactionId] = entry.duration;
          }
        }
      });
    };
    try {
      inpObserver = new PerformanceObserver(list => recordInteractions(list.getEntries()));
      inpObserver.observe({
        type: 'event',
        buffered: true,
        durationThreshold: 40,
      } as PerformanceObserverInit);
    } catch {
      /* not supported */
    }

    const computeInp = () => {
      if (inpObserver) recordInteractions(inpObserver.takeRecords());
      const values = Object.values(interactions).sort((a, b) => b - a);
      if (values.length) {
        const p98Index = Math.floor(Math.max(values.length, 10) * 0.02);
        metrics.inp = values[Math.min(p98Index, values.length - 1)];
      }
    };

    const getEntriesByType = (type: string): MetricEntry[] => {
      try {
        return (window.performance?.getEntriesByType?.(type) as MetricEntry[]) || [];
      } catch {
        return [];
      }
    };

    const applyFallbackMetrics = () => {
      if (!isInitialLoad) return;

      if (metrics.ttfb === undefined) {
        const navigation = getEntriesByType('navigation')?.[0];
        if (navigation) {
          metrics.ttfb = Math.max(navigation.responseStart - (navigation.activationStart || 0), 0);
        }
      }

      if (metrics.fcp === undefined) {
        const fcpEntry = getEntriesByType('paint')?.find(
          entry => entry.name === 'first-contentful-paint',
        );
        if (fcpEntry) {
          metrics.fcp = Math.max(fcpEntry.startTime - activationStart, 0);
        }
      }

      if (metrics.lcp === undefined) {
        const lcpEntries = getEntriesByType('largest-contentful-paint');
        const lcpEntry = lcpEntries?.[lcpEntries.length - 1];
        if (lcpEntry) {
          metrics.lcp = Math.max(lcpEntry.startTime - activationStart, 0);
        }
      }
    };

    const sendPerformance = (useBeacon = false) => {
      if (sent) return;

      applyFallbackMetrics();
      computeInp();
      metrics.duration = Math.round(performance.now() - pageStartTime);

      sent = true;
      if (timeoutId) clearTimeout(timeoutId);
      send({ ...getPayload(), ...metrics }, 'performance', useBeacon);
    };

    flushPerformance = () => {
      sendPerformance(false);
      isInitialLoad = false;
      Object.keys(metrics).forEach(k => {
        delete metrics[k];
      });
      activationStart = 0;
      pageStartTime = performance.now();
      clsSessionValue = 0;
      clsSessionEntries = [];
      interactions = {};
      sent = false;
      if (timeoutId) clearTimeout(timeoutId);
      timeoutId = setTimeout(() => sendPerformance(false), 10000);
    };
    timeoutId = setTimeout(() => sendPerformance(false), 10000);

    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'hidden') sendPerformance(true);
    });
    window.addEventListener('pagehide', () => sendPerformance(true));
  };

  /* Start */

  if (!window.umami) {
    window.umami = {
      track,
      identify,
      revenue: (
        amountOrObj: number | ({ amount: number; currency?: string } & EventData),
        currency?: string,
        customData?: EventData,
      ) => {
        if (typeof amountOrObj === 'object' && amountOrObj !== null) {
          const { amount, currency: objCurr, ...rest } = amountOrObj;
          return track('Purchase', {
            amount,
            currency: (objCurr || 'USD').toUpperCase(),
            ...rest,
          });
        }
        return track('Purchase', {
          amount: amountOrObj,
          currency: (currency || 'USD').toUpperCase(),
          ...(customData || {}),
        });
      },
      getSession: () => ({ cache, website }),
    } as UmamiTracker;
  }

  let currentUrl = normalize(href);
  let currentRef = normalize(referrer);

  let initialized = false;
  let disabled = false;
  let cache: string | undefined;
  let identity: string | undefined;
  let flushPerformance: (() => void) | undefined;
  let flushEngagement: ((isBeacon?: boolean) => void) | undefined;
  let resetEngagement: (() => void) | undefined;

  if (autoTrack && !trackingDisabled()) {
    if (document.readyState === 'complete') {
      init();
    } else {
      document.addEventListener('readystatechange', init, true);
    }
  }
})(window as TrackerWindow);
