import { Text } from '@umami/react-zen';
import { formatDistanceToNow } from 'date-fns';
import { useLocale, useTimezone } from '@/components/hooks';
import { isInvalidDate } from '@/lib/date';

export function DateDistance({ date }: { date: Date | string | number }) {
  const { formatTimezoneDate } = useTimezone();
  const { dateLocale } = useLocale();

  const d = date instanceof Date ? date : new Date(date);

  if (isInvalidDate(d)) {
    return null;
  }

  return (
    <Text title={formatTimezoneDate(d, 'PPPpp')}>
      {formatDistanceToNow(d, { addSuffix: true, locale: dateLocale })}
    </Text>
  );
}
