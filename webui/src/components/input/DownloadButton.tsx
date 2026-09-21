import { Button, Icon, Spinner, Text, Tooltip, TooltipTrigger } from '@umami/react-zen';
import Papa from 'papaparse';
import { useEffect, useRef, useState } from 'react';
import { useMessages } from '@/components/hooks';
import { Download, X } from '@/components/icons';

export function DownloadButton({
  filename = 'data',
  data,
  onClick,
}: {
  filename?: string;
  data?: any;
  onClick?: () => void;
}) {
  const { t, labels } = useMessages();
  const [isDownloading, setIsDownloading] = useState(false);
  const abortControllerRef = useRef<AbortController | null>(null);

  useEffect(() => {
    return () => {
      abortControllerRef.current?.abort();
    };
  }, []);

  const handleCancel = (e?: React.SyntheticEvent) => {
    e?.stopPropagation();
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
    }
    setIsDownloading(false);
  };

  const handleClick = async () => {
    if (isDownloading) {
      handleCancel();
      return;
    }

    if (onClick) {
      onClick();
      return;
    }

    const controller = new AbortController();
    abortControllerRef.current = controller;
    setIsDownloading(true);

    try {
      await new Promise(resolve => setTimeout(resolve, 30));
      if (controller.signal.aborted) {
        return;
      }

      const rawData = typeof data === 'function' ? await data(controller.signal) : data;
      if (controller.signal.aborted) {
        return;
      }

      downloadCsv(`${filename}.csv`, Papa.unparse(rawData), controller.signal);
    } catch (err: any) {
      if (err?.name === 'AbortError' || controller.signal.aborted) {
        return;
      }
      console.error('Download error:', err);
    } finally {
      if (abortControllerRef.current === controller) {
        abortControllerRef.current = null;
        setIsDownloading(false);
      }
    }
  };

  return isDownloading ? (
    <TooltipTrigger delay={0}>
      <Button
        variant="quiet"
        onClick={handleCancel}
        style={{ display: 'inline-flex', alignItems: 'center', gap: '4px' }}
      >
        <Icon size="sm">
          <Spinner />
        </Icon>
        <Icon size="sm">
          <X />
        </Icon>
      </Button>
      <Tooltip>
        <Text size="sm">{t(labels.cancel)}</Text>
      </Tooltip>
    </TooltipTrigger>
  ) : (
    <TooltipTrigger delay={0}>
      <Button variant="quiet" onClick={handleClick} isDisabled={!data || (Array.isArray(data) && data.length === 0)}>
        <Icon>
          <Download />
        </Icon>
      </Button>
      <Tooltip>
        <Text size="sm">{t(labels.download)}</Text>
      </Tooltip>
    </TooltipTrigger>
  );
}

function downloadCsv(filename: string, data: any, signal?: AbortSignal) {
  if (signal?.aborted) return;
  const blob = new Blob([data], { type: 'text/csv' });
  const url = URL.createObjectURL(blob);

  if (signal?.aborted) {
    URL.revokeObjectURL(url);
    return;
  }

  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();

  URL.revokeObjectURL(url);
}
