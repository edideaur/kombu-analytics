import {
  Button,
  Column,
  Form,
  FormField,
  FormSubmitButton,
  Grid,
  Icon,
  Label,
  Loading,
  Row,
  Switch,
  TextField,
} from '@umami/react-zen';
import { useEffect, useState } from 'react';
import { useApi, useConfig, useMessages, useModified } from '@/components/hooks';
import { RefreshCw } from '@/components/icons';
import { ThemeModeSelector } from '@/components/input/ThemeModeSelector';
import { getRandomChars } from '@/lib/generate';

export function SimpleShareEditForm({
  shareId,
  onSave,
  onClose,
}: {
  shareId: string;
  onSave?: (savedShare?: any) => void;
  onClose?: () => void;
}) {
  const { t, labels, getErrorMessage } = useMessages();
  const config = useConfig();
  const { get, post } = useApi();
  const { touch } = useModified();
  const { modified } = useModified('shares');
  const [share, setShare] = useState<any>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isPending, setIsPending] = useState(false);
  const [error, setError] = useState<any>(null);

  const getUrl = (slug: string) => {
    return `${config?.cloudMode ? process.env.cloudUrl : window?.location.origin}${process.env.basePath || ''}/share/${slug}`;
  };

  useEffect(() => {
    const loadShare = async () => {
      setIsLoading(true);
      try {
        const data = await get(`/share/id/${shareId}`);
        setShare(data);
      } finally {
        setIsLoading(false);
      }
    };

    loadShare();
  }, [get, modified, shareId]);

  const handleSubmit = async (data: {
    name: string;
    slug: string;
    allowFilter?: boolean;
    theme?: string;
  }) => {
    setIsPending(true);
    setError(null);

    try {
      const formattedSlug = (data.slug || '').trim().replace(/\s+/g, '-').replace(/\/+/g, '');
      const result = await post(`/share/id/${shareId}`, {
        name: data.name,
        slug: formattedSlug,
        parameters: {
          ...(share?.parameters || {}),
          allowFilter: data.allowFilter ?? true,
          theme: data.theme === 'system' ? undefined : data.theme,
        },
      });

      touch('shares');
      onSave?.(result);
      onClose?.();
    } catch (e) {
      setError(e);
    } finally {
      setIsPending(false);
    }
  };

  if (isLoading) {
    return <Loading placement="absolute" />;
  }

  return (
    <Form
      onSubmit={handleSubmit}
      error={getErrorMessage(error)}
      defaultValues={{
        name: share?.name || '',
        slug: share?.slug || '',
        allowFilter: share?.parameters?.allowFilter ?? true,
        theme: share?.parameters?.theme || 'system',
      }}
    >
      {({ watch, setValue }) => (
        <Column gap="6">
          <FormField label={t(labels.name)} name="name" rules={{ required: t(labels.required) }}>
            <TextField autoComplete="off" autoFocus />
          </FormField>
          <Grid columns="1fr auto" alignItems="end" gap>
            <FormField
              name="slug"
              label={t(labels.slug)}
              rules={{
                required: t(labels.required),
              }}
            >
              <TextField
                autoComplete="off"
                onChange={(val: string) => {
                  const formatted = val.replace(/\s+/g, '-').replace(/\/+/g, '');
                  setValue('slug', formatted, { shouldDirty: true, shouldValidate: true });
                }}
              />
            </FormField>
            <Button
              variant="quiet"
              onPress={() => setValue('slug', getRandomChars(16), { shouldDirty: true })}
            >
              <Icon>
                <RefreshCw />
              </Icon>
            </Button>
          </Grid>
          <Column>
            <Label>{t(labels.shareUrl)}</Label>
            <TextField value={getUrl(watch('slug') || '')} isReadOnly allowCopy />
          </Column>
          <Row gap="6">
            <FormField label={t(labels.filters)} name="allowFilter">
              <Switch
                isSelected={watch('allowFilter')}
                onChange={value => setValue('allowFilter', value, { shouldDirty: true })}
              >
                {t(labels.filtersEnabled)}
              </Switch>
            </FormField>
            <FormField label={t(labels.theme)} name="theme">
              <ThemeModeSelector
                value={watch('theme')}
                includeSystem
                onChange={value => setValue('theme', value, { shouldDirty: true })}
              />
            </FormField>
          </Row>
          <Row justifyContent="flex-end" paddingTop="3" gap="3">
            {onClose && (
              <Button isDisabled={isPending} onPress={onClose}>
                {t(labels.cancel)}
              </Button>
            )}
            <FormSubmitButton variant="primary" isDisabled={isPending}>
              {t(labels.save)}
            </FormSubmitButton>
          </Row>
        </Column>
      )}
    </Form>
  );
}
