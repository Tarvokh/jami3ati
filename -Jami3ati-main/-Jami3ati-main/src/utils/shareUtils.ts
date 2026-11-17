import { University, Institution } from '../types';

export function shareItem(
  item: University | Institution,
  lang: 'ar' | 'fr',
  onCopied?: (msg: string) => void
) {
  const isAr = lang === 'ar';
  const name = isAr ? item.name_ar : item.name_fr;
  const permalink = `${window.location.origin}${window.location.pathname}?item=${item.id}`;

  if (navigator.share) {
    navigator
      .share({
        title: `${name} (${item.code})`,
        text: isAr
          ? `استكشف بيانات ورابط التوثيق الموثق لـ (${name}) عبر منصة جامعتي | Jami3ati:`
          : `Consultez les informations vérifiées de (${name}) sur Jami3ati :`,
        url: permalink,
      })
      .catch(() => {
        // Fallback to clipboard if share was cancelled or failed
        navigator.clipboard.writeText(permalink).then(() => {
          onCopied?.(
            isAr
              ? `تم نسخ رابط التوثيق المباشر لـ (${item.code}) بنجاح!`
              : `Lien de partage copié pour (${item.code}) !`
          );
        });
      });
  } else {
    navigator.clipboard.writeText(permalink).then(() => {
      onCopied?.(
        isAr
          ? `تم نسخ رابط التوثيق المباشر لـ (${item.code}) بنجاح!`
          : `Lien de partage copié pour (${item.code}) !`
      );
    });
  }
}
