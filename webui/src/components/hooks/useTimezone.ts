import { formatInTimeZone, fromZonedTime, toZonedTime } from 'date-fns-tz';
import { TIMEZONE_CONFIG, TIMEZONE_LEGACY } from '@/lib/constants';
import { getTimezone } from '@/lib/date';
import { setItem } from '@/lib/storage';
import { setTimezone, useApp } from '@/store/app';
import { useLocale } from './useLocale';

const selector = (state: { timezone: string }) => state.timezone;

export function useTimezone() {
  const timezone = useApp(selector);
  const localTimeZone = getTimezone();
  const { dateLocale } = useLocale();

  const saveTimezone = (value: string) => {
    setItem(TIMEZONE_CONFIG, value);
    setTimezone(value);
  };

  const formatTimezoneDate = (date: string | Date | number, pattern: string) => {
    if (!date) return '';
    try {
      const d = typeof date === 'string' ? new Date(date) : new Date(date);
      if (isNaN(d.getTime())) return '';
      return formatInTimeZone(d, timezone, pattern, { locale: dateLocale });
    } catch {
      return '';
    }
  };

  const formatSeriesTimezone = (data: any, column: string, timezone: string) => {
    if (!Array.isArray(data)) return [];
    return data.map(item => {
      try {
        const date = new Date(item[column]);
        if (isNaN(date.getTime())) return item;

        const format = new Intl.DateTimeFormat('en-US', {
          timeZone: timezone,
          hour12: false,
          year: 'numeric',
          month: '2-digit',
          day: '2-digit',
          hour: '2-digit',
          minute: '2-digit',
          second: '2-digit',
        });

        const parts = format.formatToParts(date);
        const get = (type: string) => parts.find(p => p.type === type)?.value;

        const year = get('year');
        const month = get('month');
        const day = get('day');
        const hour = get('hour');
        const minute = get('minute');
        const second = get('second');

        return {
          ...item,
          [column]: `${year}-${month}-${day} ${hour}:${minute}:${second}`,
        };
      } catch {
        return item;
      }
    });
  };

  const toUtc = (date: Date | string | number) => {
    return fromZonedTime(date, timezone);
  };

  const fromUtc = (date: Date | string | number) => {
    return toZonedTime(date, timezone);
  };

  const localToUtc = (date: Date | string | number) => {
    return fromZonedTime(date, localTimeZone);
  };

  const localFromUtc = (date: Date | string | number) => {
    return toZonedTime(date, localTimeZone);
  };

  const canonicalizeTimezone = (timezone: string): string => {
    return TIMEZONE_LEGACY[timezone] ?? timezone;
  };

  return {
    timezone,
    localTimeZone,
    toUtc,
    fromUtc,
    localToUtc,
    localFromUtc,
    saveTimezone,
    formatTimezoneDate,
    formatSeriesTimezone,
    canonicalizeTimezone,
  };
}
