import { Button, Icon, Spinner, Tooltip, TooltipTrigger } from '@umami/react-zen';
import { useSearchParams } from 'next/navigation';
import { useEffect, useRef, useState } from 'react';
import { useApi, useMessages } from '@/components/hooks';
import { useDateParameters } from '@/components/hooks/useDateParameters';
import { useFilterParameters } from '@/components/hooks/useFilterParameters';
import { Download, X } from '@/components/icons';

export function ExportButton({ websiteId }: { websiteId: string }) {
  const { t, labels } = useMessages();
  const [isLoading, setIsLoading] = useState(false);
  const abortControllerRef = useRef<AbortController | null>(null);
  const date = useDateParameters();
  const filters = useFilterParameters();
  const searchParams = useSearchParams();
  const { get } = useApi();

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
    setIsLoading(false);
  };

  const handleClick = async () => {
    if (isLoading) {
      handleCancel();
      return;
    }

    const controller = new AbortController();
    abortControllerRef.current = controller;
    setIsLoading(true);

    try {
      const res = await get(
        `/websites/${websiteId}/export`,
        {
          ...date,
          ...filters,
          ...searchParams,
          format: 'json',
        },
        { signal: controller.signal },
      );

      if (controller.signal.aborted) {
        return;
      }

      if (res?.zip) {
        await loadZip(res.zip, controller.signal);
      } else if (typeof res === 'string') {
        downloadFile(res, `export-${websiteId}.csv`, 'text/csv; charset=utf-8', controller.signal);
      } else if (Array.isArray(res) || (res && typeof res === 'object')) {
        const jsonStr = JSON.stringify(res, null, 2);
        downloadFile(jsonStr, `export-${websiteId}.json`, 'application/json', controller.signal);
      }
    } catch (err: any) {
      if (err?.name === 'AbortError' || controller.signal.aborted || err?.isAborted) {
        return;
      }
      console.error('Export download error:', err);
    } finally {
      if (abortControllerRef.current === controller) {
        abortControllerRef.current = null;
        setIsLoading(false);
      }
    }
  };

  return isLoading ? (
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
      <Tooltip>{t(labels.cancel)}</Tooltip>
    </TooltipTrigger>
  ) : (
    <TooltipTrigger delay={0}>
      <Button variant="quiet" onClick={handleClick}>
        <Icon>
          <Download />
        </Icon>
      </Button>
      <Tooltip>{t(labels.download)}</Tooltip>
    </TooltipTrigger>
  );
}

function downloadFile(content: string, filename: string, mimeType: string, signal?: AbortSignal) {
  if (signal?.aborted) return;
  const blob = new Blob([content], { type: mimeType });
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

async function loadZip(zip: string, signal?: AbortSignal) {
  if (!zip || signal?.aborted) return;
  const binary = atob(zip);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }

  if (signal?.aborted) return;
  const blob = new Blob([bytes], { type: 'application/zip' });
  const url = URL.createObjectURL(blob);

  if (signal?.aborted) {
    URL.revokeObjectURL(url);
    return;
  }

  const a = document.createElement('a');
  a.href = url;
  a.download = 'download.zip';
  a.click();
  URL.revokeObjectURL(url);
}
