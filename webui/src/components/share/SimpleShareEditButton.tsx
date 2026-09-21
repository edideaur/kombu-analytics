import { useMessages } from '@/components/hooks';
import { Edit } from '@/components/icons';
import { DialogButton } from '@/components/input/DialogButton';
import { SimpleShareEditForm } from './SimpleShareEditForm';

export function SimpleShareEditButton({
  shareId,
  onSave,
}: {
  shareId: string;
  onSave?: (savedShare?: any) => void;
}) {
  const { t, labels } = useMessages();

  return (
    <DialogButton icon={<Edit />} title={t(labels.share)} variant="quiet" width="600px">
      {({ close }) => (
        <SimpleShareEditForm
          shareId={shareId}
          onSave={share => {
            onSave?.(share);
            close();
          }}
          onClose={close}
        />
      )}
    </DialogButton>
  );
}
