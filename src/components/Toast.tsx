import { useEffect } from "react";
import { useMusicStore, type ToastMessage } from "../store/index";
import "./Toast.css";

const TOAST_DURATION_MS = 4000;

function ToastItem({ toast }: { toast: ToastMessage }) {
  const dismissToast = useMusicStore((s) => s.dismissToast);

  useEffect(() => {
    const timer = setTimeout(() => dismissToast(toast.id), TOAST_DURATION_MS);
    return () => clearTimeout(timer);
  }, [toast.id, dismissToast]);

  return (
    <div className="toast" role="alert" aria-live="assertive">
      <span className="toast__message">{toast.message}</span>
      <button
        className="toast__close"
        aria-label="Dismiss notification"
        onClick={() => dismissToast(toast.id)}
      >
        ✕
      </button>
    </div>
  );
}

export default function ToastContainer() {
  const toasts = useMusicStore((s) => s.toasts);

  if (toasts.length === 0) return null;

  return (
    <div className="toast-container" aria-label="Notifications">
      {toasts.map((t) => (
        <ToastItem key={t.id} toast={t} />
      ))}
    </div>
  );
}
