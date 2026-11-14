//! Księżycowe jajko — jeden sekretny kod dla całego środowiska Luna.
//!
//! Kod odkrywa się w Ustawieniach (pięć szybkich kliknięć w numer wersji na
//! stronie „O Lunie”). Potem można go **wpisać** w innych aplikacjach:
//!
//! - w Plikach — zmieniając nazwę pliku lub tworząc katalog o tej nazwie,
//! - w Terminalu — wpisując kod i naciskając Enter.
//!
//! Odkrycie zapisujemy w `$XDG_STATE_HOME/luna/moon-egg`, więc każda aplikacja
//! wie, że użytkownik już je zna.
//!
//! Podziękowania są wbudowaną tabelą, a nie tłumaczeniem z katalogów `.lang`:
//! mają być widoczne **zawsze**, nawet przy nieprzetłumaczonym interfejsie.
//! Terminal składa je przez [`libluna_render::ShapedText`] (shaping + bidi +
//! fallback fontów), więc pisma złożone — arabskie, hebrajskie, dewanagari, tajskie,
//! CJK — są rysowane poprawnie, o ile system ma odpowiedni font (np. Noto Sans CJK).
//! Adres projektu jest osobną linią, żeby algorytm bidi nie przestawił URL-a.

use std::path::{Path, PathBuf};

/// Sekretny kod. Wielkość liter, spacje i myślniki nie mają znaczenia.
pub const CODE: &str = "LUNA-5-MOKOSZ";

/// Adres projektu dopisywany do każdego podziękowania.
pub const LINK: &str = "github.com/LunaDesktop";

/// Jedna linia podziękowań w danym języku.
pub struct Thanks {
    /// Nazwa języka w jego własnym zapisie (np. `Polski`, `العربية`).
    pub language: &'static str,
    /// Zdanie zapraszające do wsparcia projektu (bez adresu — ten dokłada
    /// [`thanks_line`] albo [`thanks_block`]).
    pub text: &'static str,
}

/// Podziękowania w kilkudziesięciu językach i pismach. Polska i angielska
/// otwierają listę, bo to języki projektu.
pub static THANKS: &[Thanks] = &[
    Thanks {
        language: "Polski",
        text: "Dziękujemy, że jesteś z nami. Jeśli chcesz wspomóc rozwój Luny, napisz na GitHubie:",
    },
    Thanks {
        language: "English",
        text: "Thank you for walking with us. To help Luna grow, write to us on GitHub:",
    },
    Thanks {
        language: "Deutsch",
        text: "Danke, dass du dabei bist. Wenn du Luna unterstützen möchtest, schreib uns auf GitHub:",
    },
    Thanks {
        language: "Français",
        text: "Merci de votre soutien. Pour aider Luna à grandir, écrivez-nous sur GitHub :",
    },
    Thanks {
        language: "Español",
        text: "Gracias por acompañarnos. Si quieres apoyar el desarrollo de Luna, escríbenos en GitHub:",
    },
    Thanks {
        language: "Italiano",
        text: "Grazie per essere con noi. Se vuoi sostenere lo sviluppo di Luna, scrivici su GitHub:",
    },
    Thanks {
        language: "Português",
        text: "Obrigado por nos acompanhar. Se quiser apoiar o desenvolvimento da Luna, escreva-nos no GitHub:",
    },
    Thanks {
        language: "Nederlands",
        text: "Bedankt dat je erbij bent. Wil je Luna helpen groeien? Schrijf ons op GitHub:",
    },
    Thanks {
        language: "Svenska",
        text: "Tack för att du är med. Vill du stötta Lunautvecklingen? Skriv till oss på GitHub:",
    },
    Thanks {
        language: "Norsk",
        text: "Takk for at du er med. Vil du støtte utviklingen av Luna? Skriv til oss på GitHub:",
    },
    Thanks {
        language: "Dansk",
        text: "Tak, fordi du er med. Vil du støtte udviklingen af Luna? Skriv til os på GitHub:",
    },
    Thanks {
        language: "Suomi",
        text: "Kiitos, että olet mukana. Jos haluat tukea Lunan kehitystä, kirjoita meille GitHubissa:",
    },
    Thanks {
        language: "Íslenska",
        text: "Takk fyrir að fylgja okkur. Ef þú vilt styðja þróun Lunu skaltu skrifa okkur á GitHub:",
    },
    Thanks {
        language: "Čeština",
        text: "Děkujeme, že jste s námi. Chcete-li podpořit vývoj Luny, napište nám na GitHub:",
    },
    Thanks {
        language: "Slovenčina",
        text: "Ďakujeme, že ste s nami. Ak chcete podporiť vývoj Luny, napíšte nám na GitHub:",
    },
    Thanks {
        language: "Magyar",
        text: "Köszönjük, hogy velünk vagy. Ha támogatnád a Luna fejlesztését, írj nekünk a GitHubon:",
    },
    Thanks {
        language: "Română",
        text: "Mulțumim că ești alături de noi. Dacă vrei să sprijini dezvoltarea Luna, scrie-ne pe GitHub:",
    },
    Thanks {
        language: "Български",
        text: "Благодарим, че сте с нас. Ако искате да подкрепите развитието на Luna, пишете ни в GitHub:",
    },
    Thanks {
        language: "Русский",
        text: "Спасибо, что вы с нами. Если хотите поддержать развитие Luna, напишите нам на GitHub:",
    },
    Thanks {
        language: "Українська",
        text: "Дякуємо, що ви з нами. Якщо хочете підтримати розвиток Luna, напишіть нам на GitHub:",
    },
    Thanks {
        language: "Беларуская",
        text: "Дзякуй, што вы з намі. Калі вы хочаце падтрымаць развіццё Luna, напішыце нам на GitHub:",
    },
    Thanks {
        language: "Српски",
        text: "Хвала што сте са нама. Ако желите да подржите развој Luna, пишите нам на GitHub:",
    },
    Thanks {
        language: "Hrvatski",
        text: "Hvala što ste s nama. Ako želite podržati razvoj Lune, pišite nam na GitHubu:",
    },
    Thanks {
        language: "Bosanski",
        text: "Hvala što ste s nama. Ako želite podržati razvoj Lune, pišite nam na GitHubu:",
    },
    Thanks {
        language: "Македонски",
        text: "Ви благодариме што сте со нас. Ако сакате да го поддржите развојот на Luna, пишете ни на GitHub:",
    },
    Thanks {
        language: "Slovenščina",
        text: "Hvala, ker ste z nami. Če želite podpreti razvoj Lune, pišite nam na GitHubu:",
    },
    Thanks {
        language: "Lietuvių",
        text: "Ačiū, kad esate su mumis. Jei norite paremti Luna kūrimą, rašykite mums GitHub:",
    },
    Thanks {
        language: "Latviešu",
        text: "Paldies, ka esat kopā ar mums. Ja vēlaties atbalstīt Luna izstrādi, rakstiet mums GitHub:",
    },
    Thanks {
        language: "Eesti",
        text: "Aitäh, et olete meiega. Kui soovite Luna arendust toetada, kirjutage meile GitHubis:",
    },
    Thanks {
        language: "Ελληνικά",
        text: "Ευχαριστούμε που είστε μαζί μας. Αν θέλετε να στηρίξετε την ανάπτυξη του Luna, γράψτε μας στο GitHub:",
    },
    Thanks {
        language: "Türkçe",
        text: "Yanımızda olduğunuz için teşekkürler. Luna'nın gelişimini desteklemek isterseniz GitHub'da bize yazın:",
    },
    Thanks {
        language: "Azərbaycan",
        text: "Bizimlə olduğunuz üçün təşəkkür edirik. Luna inkişafını dəstəkləmək istəyirsinizsə, GitHub-da bizə yazın:",
    },
    Thanks {
        language: "Қазақша",
        text: "Бізбен бірге болғаныңызға рахмет. Luna дамуын қолдағыңыз келсе, GitHub-та бізге жазыңыз:",
    },
    Thanks {
        language: "Кыргызча",
        text: "Биз менен болгонуңуз үчүн рахмат. Luna өнүгүүсүн колдоого кааласаңыз, GitHub аркылуу бизге жазыңыз:",
    },
    Thanks {
        language: "Oʻzbek",
        text: "Biz bilan bo'lganingiz uchun rahmat. Luna rivojini qo'llab-quvvatlamoqchi bo'lsangiz, GitHub'da bizga yozing:",
    },
    Thanks {
        language: "Тоҷикӣ",
        text: "Ташаккур, ки шумо бо мо ҳастед. Агар мехоҳед рушди Luna-ро дастгирӣ кунед, дар GitHub ба мо нависед:",
    },
    Thanks {
        language: "Монгол",
        text: "Бидэнтэй хамт байгаа танд баярлалаа. Luna-гийн хөгжлийг дэмжихийг хүсвэл GitHub-аар бидэнд бичээрэй:",
    },
    Thanks {
        language: "ქართული",
        text: "გმადლობთ, რომ ჩვენთან ხართ. თუ გსურთ Luna-ს განვითარების მხარდაჭერა, მოგვწერეთ GitHub-ზე:",
    },
    Thanks {
        language: "Հայերեն",
        text: "Շնորհակալություն, որ մեզ հետ եք։ Եթե ցանկանում եք աջակցել Luna-ի զարգացմանը, գրեք մեզ GitHub-ում՝",
    },
    Thanks {
        language: "עברית",
        text: "תודה שאת/ה איתנו. אם תרצה/י לתמוך בפיתוח של Luna, כתוב/כתבי לנו ב-GitHub:",
    },
    Thanks {
        language: "العربية",
        text: "شكرًا لكونك معنا. إذا أردت دعم تطوير Luna، راسلنا على GitHub:",
    },
    Thanks {
        language: "فارسی",
        text: "سپاسگزاریم که با ما هستید. اگر می‌خواهید از توسعهٔ Luna پشتیبانی کنید، در GitHub به ما بنویسید:",
    },
    Thanks {
        language: "اردو",
        text: "ہمارے ساتھ ہونے کا شکریہ۔ اگر آپ لونا کی ترقی میں مدد کرنا چاہتے ہیں تو GitHub پر ہمیں لکھیں:",
    },
    Thanks {
        language: "हिन्दी",
        text: "हमारे साथ होने के लिए धन्यवाद। अगर आप Luna के विकास में सहयोग करना चाहें, तो GitHub पर हमें लिखें:",
    },
    Thanks {
        language: "नेपाली",
        text: "हामीसँग हुनुभएकोमा धन्यवाद। Luna को विकासमा सहयोग गर्न चाहनुहुन्छ भने GitHub मा हामीलाई लेख्नुहोस्:",
    },
    Thanks {
        language: "বাংলা",
        text: "আমাদের সঙ্গে থাকার জন্য ধন্যবাদ। আপনি যদি Luna-এর উন্নয়নে সহায়তা করতে চান, GitHub-এ আমাদের লিখুন:",
    },
    Thanks {
        language: "தமிழ்",
        text: "எங்களுடன் இருப்பதற்கு நன்றி. Luna-வின் வளர்ச்சிக்கு உதவ விரும்பினால், GitHub-இல் எங்களுக்கு எழுதுங்கள்:",
    },
    Thanks {
        language: "తెలుగు",
        text: "మాతో ఉన్నందుకు ధన్యవాదాలు. Luna అభివృద్ధికి తోడ్పడాలనుకుంటే, GitHubలో మాకు రాయండి:",
    },
    Thanks {
        language: "ಕನ್ನಡ",
        text: "ನಮ್ಮೊಂದಿಗೆ ಇರುವುದಕ್ಕೆ ಧನ್ಯವಾದಗಳು. Luna ಅಭಿವೃದ್ಧಿಯನ್ನು ಬೆಂಬಲಿಸಲು ಬಯಸಿದರೆ, GitHub ನಲ್ಲಿ ನಮಗೆ ಬರೆಯಿರಿ:",
    },
    Thanks {
        language: "മലയാളം",
        text: "ഞങ്ങളോടൊപ്പം ഉള്ളതിന് നന്ദി. Luna വികസനത്തെ പിന്തുണയ്ക്കാൻ ആഗ്രഹിക്കുന്നുവെങ്കിൽ, GitHub ൽ ഞങ്ങൾക്ക് എഴുതുക:",
    },
    Thanks {
        language: "සිංහල",
        text: "අප සමඟ සිටින ගමනට ස්තූතියි. Luna සංවර්ධනයට සහාය දීමට අවශ්‍ය නම්, GitHub හරහා අපට ලියන්න:",
    },
    Thanks {
        language: "ไทย",
        text: "ขอบคุณที่อยู่กับเรา หากต้องการสนับสนุนการพัฒนา Luna เขียนหาเราได้ที่ GitHub:",
    },
    Thanks {
        language: "ລາວ",
        text: "ຂອບໃຈທີ່ຢູ່ກັບພວກເຮົາ. ຖ້າຕ້ອງການສະໜັບສະໜູນການພັດທະນາ Luna, ຂຽນຫາພວກເຮົາທີ່ GitHub:",
    },
    Thanks {
        language: "ខ្មែរ",
        text: "សូមអរគុណដែលបាននៅជាមួយយើង។ ប្រសិនបើអ្នកចង់គាំទ្រការអភិវឌ្ឍ Luna សូមសរសេរមកយើងតាម GitHub៖",
    },
    Thanks {
        language: "မြန်မာ",
        text: "ကျွန်ုပ်တို့နှင့်အတူရှိသည့်အတွက် ကျေးဇူးတင်ပါသည်။ Luna ဖွံ့ဖြိုးတိုးတက်ရေးကို ပံ့ပိုးလိုပါက GitHub တွင် ကျွန်ုပ်တို့ထံ စာရေးပါ-",
    },
    Thanks {
        language: "中文（简体）",
        text: "感谢你与我们同行。如果你想支持 Luna 的开发，请在 GitHub 上联系我们：",
    },
    Thanks {
        language: "中文（繁體）",
        text: "感謝你與我們同行。如果你想支持 Luna 的開發，請在 GitHub 上聯絡我們：",
    },
    Thanks {
        language: "日本語",
        text: "私たちと共に歩んでくれてありがとう。Luna の発展を支えたい方は、GitHub でご連絡ください：",
    },
    Thanks {
        language: "한국어",
        text: "함께해 주셔서 감사합니다. Luna의 발전을 돕고 싶다면 GitHub에서 저희에게 알려 주세요:",
    },
    Thanks {
        language: "አማርኛ",
        text: "ከእኛ ጋር ስለሆኑ እናመሰግናለን። የ Luna ልማትን መደገፍ ከፈለጉ በ GitHub ላይ ይፃፉልን፦",
    },
    Thanks {
        language: "Esperanto",
        text: "Dankon, ke vi estas kun ni. Se vi volas subteni la evoluon de Luna, skribu al ni ĉe GitHub:",
    },
    Thanks {
        language: "Català",
        text: "Gràcies per acompanyar-nos. Si vols donar suport al desenvolupament de Luna, escriu-nos a GitHub:",
    },
    Thanks {
        language: "Galego",
        text: "Grazas por acompañarnos. Se queres apoiar o desenvolvemento de Luna, escríbenos en GitHub:",
    },
    Thanks {
        language: "Euskara",
        text: "Eskerrik asko gurekin egoteagatik. Lunaren garapena lagundu nahi baduzu, idatzi iezaguzu GitHub-en:",
    },
    Thanks {
        language: "Afrikaans",
        text: "Dankie dat jy by ons is. Om Luna se ontwikkeling te steun, skryf aan ons op GitHub:",
    },
    Thanks {
        language: "Kiswahili",
        text: "Asante kwa kuwa nasi. Ikiwa unataka kusaidia maendeleo ya Luna, tuandikie kwenye GitHub:",
    },
    Thanks {
        language: "Bahasa Indonesia",
        text: "Terima kasih telah bersama kami. Jika ingin mendukung pengembangan Luna, tulis kepada kami di GitHub:",
    },
    Thanks {
        language: "Bahasa Melayu",
        text: "Terima kasih kerana bersama kami. Jika anda ingin menyokong pembangunan Luna, tulis kepada kami di GitHub:",
    },
    Thanks {
        language: "Filipino",
        text: "Salamat sa pagsama sa amin. Kung gusto mong suportahan ang pag-unlad ng Luna, sumulat sa amin sa GitHub:",
    },
    Thanks {
        language: "Tiếng Việt",
        text: "Cảm ơn bạn đã đồng hành. Nếu muốn ủng hộ sự phát triển của Luna, hãy viết cho chúng tôi trên GitHub:",
    },
    Thanks {
        language: "Gaeilge",
        text: "Go raibh maith agat as a bheith linn. Más mian leat tacú le forbairt Luna, scríobh chugainn ar GitHub:",
    },
    Thanks {
        language: "Cymraeg",
        text: "Diolch am fod gyda ni. Os hoffech gefnogi datblygiad Luna, ysgrifennwch atom ar GitHub:",
    },
    Thanks {
        language: "Malti",
        text: "Grazzi talli qiegħed magħna. Jekk trid tappoġġja l-iżvilupp ta' Luna, iktbilna fuq GitHub:",
    },
    Thanks {
        language: "Shqip",
        text: "Faleminderit që jeni me ne. Nëse dëshironi të mbështesni zhvillimin e Luna, shkruajini nesh në GitHub:",
    },
];

/// Czy tekst jest sekretnym kodem.
///
/// Porównanie ignoruje wielkość liter oraz wszystko, co nie jest literą lub
/// cyfrą — dzięki temu działa zarówno `LUNA-5-MOKOSZ`, jak i `luna 5 mokosz`.
pub fn matches(input: &str) -> bool {
    !input.is_empty() && normalize(input) == normalize(CODE)
}

/// Podziękowanie w jednej linii (okna dialogowe, gdzie nie ma miejsca).
pub fn thanks_line(entry: &Thanks) -> String {
    format!("{} {}", entry.text, LINK)
}

/// Podziękowanie z adresem w **osobnej linii** — bezpieczne dla pism RTL.
pub fn thanks_block(entry: &Thanks) -> String {
    format!("{}\n{}", entry.text, LINK)
}

/// Ścieżka znacznika odkrycia (`$XDG_STATE_HOME/luna/moon-egg`).
pub fn state_file() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("state"))
        })?;
    Some(base.join("luna").join("moon-egg"))
}

/// Czy jajko zostało już odkryte w tej sesji użytkownika.
pub fn unlocked() -> bool {
    state_file().is_some_and(|path| path.is_file())
}

/// Zapisuje odkrycie. Zwraca `true`, gdy to **nowe** odkrycie (wcześniej brak
/// znacznika); powtórne wywołanie nic nie zmienia i zwraca `false`.
pub fn unlock() -> bool {
    let Some(path) = state_file() else {
        return false;
    };
    unlock_at(&path)
}

fn unlock_at(path: &Path) -> bool {
    if path.is_file() {
        return false;
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let text = format!("code = {CODE}\ndiscovered = {stamp}\n");
    crate::config::save_atomic(path, &text).is_ok()
}

fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_uppercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_code_with_spacing_case_and_dashes() {
        assert!(matches(CODE));
        assert!(matches("luna-5-mokosz"));
        assert!(matches("  LUNA 5 MOKOSZ  "));
        assert!(matches("Luna5Mokosz"));
    }

    #[test]
    fn rejects_lookalikes() {
        assert!(!matches(""));
        assert!(!matches("LUNA-5"));
        assert!(!matches("LUNA-6-MOKOSZ"));
        assert!(!matches("MOKOSZ"));
    }

    #[test]
    fn thanks_cover_diverse_scripts_and_keep_link() {
        assert!(THANKS.len() >= 60, "szeroka lista języków");
        for required in ["العربية", "עברית", "हिन्दी", "ไทย", "日本語", "한국어"]
        {
            assert!(
                THANKS.iter().any(|entry| entry.language == required),
                "brak języka {required}"
            );
        }
        for entry in THANKS {
            assert!(!entry.language.is_empty());
            assert!(!entry.text.contains("{link}"), "{}", entry.language);
            assert!(thanks_line(entry).contains(LINK), "{}", entry.language);
            let block = thanks_block(entry);
            assert!(block.contains(LINK), "{}", entry.language);
            assert!(block.contains('\n'), "{}", entry.language);
        }
    }

    #[test]
    fn unlock_is_idempotent_and_uses_marker() {
        let dir = std::env::temp_dir().join(format!("luna-egg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("luna").join("moon-egg");

        assert!(!path.is_file());
        assert!(unlock_at(&path), "pierwsze odkrycie zapisuje znacznik");
        assert!(path.is_file());
        assert!(!unlock_at(&path), "powtórne odkrycie nie zmienia stanu");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(CODE));

        let _ = std::fs::remove_dir_all(dir);
    }
}
