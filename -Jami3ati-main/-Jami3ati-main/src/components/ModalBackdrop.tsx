import React, { ReactNode } from 'react';

interface Props {
  onClose: () => void;
  children: ReactNode;
  ariaLabelledBy?: string;
  className?: string;
}

/**
 * Consistent fixed-position modal backdrop component with rgba(0,0,0,0.5) overlay
 * and centered content alignment.
 */
export const ModalBackdrop: React.FC<Props> = ({
  onClose,
  children,
  ariaLabelledBy,
  className = '',
}) => {
  return (
    <div
      className={`fixed inset-0 flex items-center justify-center z-50 bg-[rgba(0,0,0,0.5)] p-3 sm:p-6 select-none animate-in fade-in duration-150 ${className}`}
      role="dialog"
      aria-modal="true"
      aria-labelledby={ariaLabelledBy}
      onClick={(e) => {
        if (e.target === e.currentTarget) {
          onClose();
        }
      }}
    >
      {children}
    </div>
  );
};
