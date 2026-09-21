import {
  Button,
  Checkbox,
  Column,
  Form,
  FormField,
  FormSubmitButton,
  Grid,
  Icon,
  Label,
  Row,
  TextField,
} from '@umami/react-zen';
import { useState } from 'react';
import { useApi, useConfig, useMessages, useModified } from '@/components/hooks';
import { RefreshCw } from '@/components/icons';
import { ThemeModeSelector } from '@/components/input/ThemeModeSelector';
import { getRandomChars } from '@/lib/generate';

export function BoardShareCreateForm({
  boardId,
  onSave,
  onCancel,
}: {
  boardId: string;
  onSave?: () => void;
  onCancel?: () => void;
}) {
  const { post } = useApi();
  const config = useConfig();
  const { touch } = useModified();
  const { t, labels, getErrorMessage } = useMessages();
  const [isPending, setIsPending] = useState(false);
  const [error, setError] = useState<any>(null);

  const getUrl = (slug: string) => {
    return `${config?.cloudMode ? process.env.cloudUrl : window?.location.origin}${process.env.basePath || ''}/share/${slug}`;
  };

  const handleSubmit = async (data: {
    name: string;
    slug?: string;
    allowFilter?: boolean;
    theme?: string;
  }) => {
    setIsPending(true);
    setError(null);

    try {
      const formattedSlug = (data.slug || '').trim().replace(/\s+/g, '-').replace(/\/+/g, '');
      await post(`/boards/${boardId}/shares`, {
        name: data.name,
        slug: formattedSlug || undefined,
        parameters: {
          allowFilter: data.allowFilter ?? true,
          theme: data.theme === 'system' ? undefined : data.theme,
        },
      });

      touch('shares');
      onSave?.();
    } catch (e) {
      setError(e);
    } finally {
      setIsPending(false);
    }
  };

  return (
    <Form
      onSubmit={handleSubmit}
      error={getErrorMessage(error)}
      defaultValues={{ name: '', slug: getRandomChars(16), allowFilter: true, theme: 'system' }}
    >
      {({ watch, setValue }) => (
        <Column gap="4">
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
          <FormField name="allowFilter">
            <Checkbox>{t(labels.filters)}</Checkbox>
          </FormField>
          <FormField label={t(labels.theme)} name="theme">
            <ThemeModeSelector
              value={watch('theme')}
              includeSystem
              onChange={value => setValue('theme', value, { shouldDirty: true })}
            />
          </FormField>
          <Row justifyContent="flex-end" gap="3">
            {onCancel && (
              <Button isDisabled={isPending} onPress={onCancel}>
                {t(labels.cancel)}
              </Button>
            )}
            <FormSubmitButton variant="primary" isDisabled={isPending}>
              {t(labels.add)}
            </FormSubmitButton>
          </Row>
        </Column>
      )}
    </Form>
  );
}
