import { Button, Dialog, DialogTrigger, Icon, Modal, Text, useToast } from '@umami/react-zen';
import { useMessages } from '@/components/hooks';
import { User } from '@/components/icons';
import { UsernameEditForm } from './UsernameEditForm';

export function UsernameChangeButton() {
  const { t, labels, messages } = useMessages();
  const { toast } = useToast();

  const handleSave = () => {
    toast(t(messages.saved));
  };

  return (
    <DialogTrigger>
      <Button>
        <Icon>
          <User />
        </Icon>
        <Text>{t(labels.edit)}</Text>
      </Button>
      <Modal>
        <Dialog title={t(labels.username)} style={{ width: 400 }}>
          {({ close }) => <UsernameEditForm onSave={handleSave} onClose={close} />}
        </Dialog>
      </Modal>
    </DialogTrigger>
  );
}
