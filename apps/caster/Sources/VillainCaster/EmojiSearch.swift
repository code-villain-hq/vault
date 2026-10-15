import Foundation

/// "emoji <query>" → list of matching emoji, ⏎ copies.
/// Curated offline list; extend `entries` as needed.
enum EmojiSearch {
    struct Entry {
        let emoji: String
        let name: String
        let keywords: String
    }

    static func search(_ text: String) -> [ResultItem]? {
        let lower = text.lowercased()
        guard lower == "emoji" || lower.hasPrefix("emoji ") else { return nil }
        let query = String(lower.dropFirst(5)).trimmingCharacters(in: .whitespaces)
        guard !query.isEmpty else {
            return [ResultItem(icon: nil, title: "😀 Type to search emoji",
                               subtitle: "e.g. \"emoji shrug\"", action: nil)]
        }

        let queryChars = Array(query)
        let scored = searchable.compactMap { entry, target -> (score: Int, entry: Entry)? in
            guard let score = Fuzzy.score(queryChars: queryChars, targetChars: target)
            else { return nil }
            return (score, entry)
        }
        return scored
            .sorted { $0.score > $1.score }
            .prefix(8)
            .map { _, entry in
                ResultItem(icon: nil,
                           title: "\(entry.emoji)  \(entry.name.capitalized)",
                           subtitle: "⏎ copies",
                           action: { Clipboard.copy(entry.emoji) })
            }
    }

    /// Search targets precomputed once — names and keywords are already
    /// lowercase in the list below.
    private static let searchable: [(entry: Entry, target: [Character])] =
        entries.map { ($0, Array($0.name + " " + $0.keywords)) }

    private static let entries: [Entry] = [
        // Smileys
        Entry(emoji: "😀", name: "grinning face", keywords: "smile happy"),
        Entry(emoji: "😂", name: "tears of joy", keywords: "laugh lol funny"),
        Entry(emoji: "🤣", name: "rolling on the floor laughing", keywords: "rofl lol"),
        Entry(emoji: "😊", name: "smiling face", keywords: "blush happy"),
        Entry(emoji: "😉", name: "winking face", keywords: "wink"),
        Entry(emoji: "😍", name: "heart eyes", keywords: "love adore"),
        Entry(emoji: "😘", name: "face blowing kiss", keywords: "kiss love"),
        Entry(emoji: "😎", name: "sunglasses", keywords: "cool"),
        Entry(emoji: "🤔", name: "thinking face", keywords: "hmm wonder"),
        Entry(emoji: "😅", name: "sweat smile", keywords: "phew nervous laugh"),
        Entry(emoji: "😢", name: "crying face", keywords: "sad tear"),
        Entry(emoji: "😭", name: "loudly crying", keywords: "sob sad bawling"),
        Entry(emoji: "😡", name: "angry face", keywords: "mad rage"),
        Entry(emoji: "🤯", name: "exploding head", keywords: "mind blown wow"),
        Entry(emoji: "😱", name: "screaming in fear", keywords: "shock scared"),
        Entry(emoji: "🥳", name: "partying face", keywords: "party celebrate birthday"),
        Entry(emoji: "😴", name: "sleeping face", keywords: "zzz tired sleep"),
        Entry(emoji: "🤒", name: "face with thermometer", keywords: "sick ill fever"),
        Entry(emoji: "🤢", name: "nauseated face", keywords: "sick gross vomit"),
        Entry(emoji: "🙃", name: "upside down face", keywords: "silly sarcasm"),
        Entry(emoji: "😬", name: "grimacing face", keywords: "awkward eek"),
        Entry(emoji: "🫠", name: "melting face", keywords: "embarrassed hot dread"),
        Entry(emoji: "🤡", name: "clown face", keywords: "joke fool"),
        Entry(emoji: "💀", name: "skull", keywords: "dead death dying"),
        Entry(emoji: "👻", name: "ghost", keywords: "boo spooky halloween"),
        Entry(emoji: "💩", name: "pile of poo", keywords: "poop crap"),
        Entry(emoji: "🤖", name: "robot", keywords: "bot ai machine"),
        Entry(emoji: "👽", name: "alien", keywords: "ufo extraterrestrial"),
        // Gestures & people
        Entry(emoji: "🤷", name: "shrug", keywords: "person shrugging dunno whatever"),
        Entry(emoji: "🤦", name: "facepalm", keywords: "person facepalming doh"),
        Entry(emoji: "👍", name: "thumbs up", keywords: "like ok yes approve +1"),
        Entry(emoji: "👎", name: "thumbs down", keywords: "dislike no -1"),
        Entry(emoji: "👏", name: "clapping hands", keywords: "applause bravo"),
        Entry(emoji: "🙏", name: "folded hands", keywords: "pray please thanks thank you"),
        Entry(emoji: "👋", name: "waving hand", keywords: "hello hi bye wave"),
        Entry(emoji: "🤝", name: "handshake", keywords: "deal agreement"),
        Entry(emoji: "👌", name: "ok hand", keywords: "perfect fine"),
        Entry(emoji: "✌️", name: "victory hand", keywords: "peace two"),
        Entry(emoji: "🤞", name: "crossed fingers", keywords: "luck hope"),
        Entry(emoji: "🖕", name: "middle finger", keywords: "rude fu"),
        Entry(emoji: "💪", name: "flexed biceps", keywords: "muscle strong gym"),
        Entry(emoji: "🧠", name: "brain", keywords: "smart mind think"),
        Entry(emoji: "👀", name: "eyes", keywords: "look watch see sus"),
        Entry(emoji: "🫡", name: "saluting face", keywords: "salute yes sir respect"),
        Entry(emoji: "🤌", name: "pinched fingers", keywords: "italian chef kiss"),
        Entry(emoji: "✍️", name: "writing hand", keywords: "write note sign"),
        // Hearts
        Entry(emoji: "❤️", name: "red heart", keywords: "love"),
        Entry(emoji: "🧡", name: "orange heart", keywords: "love"),
        Entry(emoji: "💛", name: "yellow heart", keywords: "love"),
        Entry(emoji: "💚", name: "green heart", keywords: "love"),
        Entry(emoji: "💙", name: "blue heart", keywords: "love"),
        Entry(emoji: "💜", name: "purple heart", keywords: "love"),
        Entry(emoji: "🖤", name: "black heart", keywords: "love dark"),
        Entry(emoji: "💔", name: "broken heart", keywords: "sad breakup"),
        Entry(emoji: "❤️‍🔥", name: "heart on fire", keywords: "love passion"),
        // Celebration & symbols
        Entry(emoji: "🎉", name: "party popper", keywords: "celebrate tada congrats"),
        Entry(emoji: "🎊", name: "confetti ball", keywords: "celebrate party"),
        Entry(emoji: "🎂", name: "birthday cake", keywords: "celebrate bday"),
        Entry(emoji: "🎁", name: "wrapped gift", keywords: "present birthday"),
        Entry(emoji: "🎈", name: "balloon", keywords: "party birthday"),
        Entry(emoji: "🏆", name: "trophy", keywords: "win winner champion prize"),
        Entry(emoji: "🥇", name: "gold medal", keywords: "first winner"),
        Entry(emoji: "🔥", name: "fire", keywords: "hot lit flame"),
        Entry(emoji: "✨", name: "sparkles", keywords: "shiny magic new"),
        Entry(emoji: "⭐", name: "star", keywords: "favorite"),
        Entry(emoji: "🌟", name: "glowing star", keywords: "star shine"),
        Entry(emoji: "🚀", name: "rocket", keywords: "launch ship fast space"),
        Entry(emoji: "💯", name: "hundred points", keywords: "100 perfect score"),
        Entry(emoji: "✅", name: "check mark button", keywords: "done yes ok correct"),
        Entry(emoji: "❌", name: "cross mark", keywords: "no wrong delete x"),
        Entry(emoji: "⚠️", name: "warning", keywords: "caution alert"),
        Entry(emoji: "❓", name: "question mark", keywords: "what help"),
        Entry(emoji: "❗", name: "exclamation mark", keywords: "important alert bang"),
        Entry(emoji: "💡", name: "light bulb", keywords: "idea"),
        Entry(emoji: "🔴", name: "red circle", keywords: "dot record"),
        Entry(emoji: "🟢", name: "green circle", keywords: "dot online go"),
        Entry(emoji: "🟡", name: "yellow circle", keywords: "dot warning"),
        Entry(emoji: "♻️", name: "recycling", keywords: "recycle green"),
        Entry(emoji: "⚡", name: "high voltage", keywords: "zap lightning fast electric"),
        // Work & tech
        Entry(emoji: "💻", name: "laptop", keywords: "computer mac code"),
        Entry(emoji: "⌨️", name: "keyboard", keywords: "type"),
        Entry(emoji: "🖥️", name: "desktop computer", keywords: "monitor screen"),
        Entry(emoji: "📱", name: "mobile phone", keywords: "iphone smartphone"),
        Entry(emoji: "🐛", name: "bug", keywords: "insect error"),
        Entry(emoji: "🔧", name: "wrench", keywords: "fix tool repair"),
        Entry(emoji: "🔨", name: "hammer", keywords: "build tool"),
        Entry(emoji: "⚙️", name: "gear", keywords: "settings config"),
        Entry(emoji: "🔒", name: "locked", keywords: "lock secure private"),
        Entry(emoji: "🔑", name: "key", keywords: "password unlock"),
        Entry(emoji: "🔍", name: "magnifying glass", keywords: "search find zoom"),
        Entry(emoji: "📌", name: "pushpin", keywords: "pin location"),
        Entry(emoji: "🔗", name: "link", keywords: "url chain"),
        Entry(emoji: "📎", name: "paperclip", keywords: "attach"),
        Entry(emoji: "📝", name: "memo", keywords: "note write document"),
        Entry(emoji: "📄", name: "page", keywords: "document file"),
        Entry(emoji: "📚", name: "books", keywords: "read library study"),
        Entry(emoji: "📅", name: "calendar", keywords: "date schedule"),
        Entry(emoji: "⏰", name: "alarm clock", keywords: "time wake"),
        Entry(emoji: "⌛", name: "hourglass", keywords: "time wait"),
        Entry(emoji: "📧", name: "email", keywords: "mail message"),
        Entry(emoji: "📦", name: "package", keywords: "box delivery ship"),
        Entry(emoji: "📈", name: "chart increasing", keywords: "graph up growth"),
        Entry(emoji: "📉", name: "chart decreasing", keywords: "graph down loss"),
        Entry(emoji: "💰", name: "money bag", keywords: "cash rich dollar"),
        Entry(emoji: "💸", name: "money with wings", keywords: "spend expensive lost"),
        Entry(emoji: "🗑️", name: "wastebasket", keywords: "trash delete bin"),
        // Food & drink
        Entry(emoji: "☕", name: "coffee", keywords: "hot beverage cafe"),
        Entry(emoji: "🍺", name: "beer mug", keywords: "drink pint"),
        Entry(emoji: "🍷", name: "wine glass", keywords: "drink"),
        Entry(emoji: "🍕", name: "pizza", keywords: "food slice"),
        Entry(emoji: "🍔", name: "hamburger", keywords: "food burger"),
        Entry(emoji: "🌮", name: "taco", keywords: "food mexican"),
        Entry(emoji: "🍣", name: "sushi", keywords: "food japanese"),
        Entry(emoji: "🍰", name: "shortcake", keywords: "cake dessert sweet"),
        Entry(emoji: "🍦", name: "ice cream", keywords: "dessert soft serve"),
        Entry(emoji: "🍎", name: "red apple", keywords: "fruit"),
        Entry(emoji: "🍌", name: "banana", keywords: "fruit"),
        Entry(emoji: "🥑", name: "avocado", keywords: "fruit guacamole"),
        Entry(emoji: "🥕", name: "carrot", keywords: "vegetable"),
        Entry(emoji: "🍿", name: "popcorn", keywords: "movie snack"),
        // Animals & nature
        Entry(emoji: "🐶", name: "dog face", keywords: "puppy pet"),
        Entry(emoji: "🐱", name: "cat face", keywords: "kitten pet"),
        Entry(emoji: "🐭", name: "mouse face", keywords: "rodent"),
        Entry(emoji: "🦊", name: "fox", keywords: "animal"),
        Entry(emoji: "🐻", name: "bear", keywords: "animal"),
        Entry(emoji: "🐼", name: "panda", keywords: "animal"),
        Entry(emoji: "🐵", name: "monkey face", keywords: "ape"),
        Entry(emoji: "🦄", name: "unicorn", keywords: "magic startup"),
        Entry(emoji: "🐍", name: "snake", keywords: "python"),
        Entry(emoji: "🐢", name: "turtle", keywords: "slow tortoise"),
        Entry(emoji: "🦀", name: "crab", keywords: "rust"),
        Entry(emoji: "🐳", name: "spouting whale", keywords: "docker sea"),
        Entry(emoji: "🦋", name: "butterfly", keywords: "insect pretty"),
        Entry(emoji: "🌹", name: "rose", keywords: "flower love"),
        Entry(emoji: "🌻", name: "sunflower", keywords: "flower"),
        Entry(emoji: "🌲", name: "evergreen tree", keywords: "forest nature"),
        // Weather & sky
        Entry(emoji: "☀️", name: "sun", keywords: "sunny weather"),
        Entry(emoji: "🌙", name: "crescent moon", keywords: "night"),
        Entry(emoji: "☁️", name: "cloud", keywords: "weather overcast"),
        Entry(emoji: "🌧️", name: "cloud with rain", keywords: "weather raining"),
        Entry(emoji: "❄️", name: "snowflake", keywords: "snow cold winter"),
        Entry(emoji: "🌈", name: "rainbow", keywords: "pride color"),
        Entry(emoji: "🌊", name: "water wave", keywords: "ocean sea surf"),
        Entry(emoji: "🌍", name: "globe europe africa", keywords: "earth world planet"),
        // Travel
        Entry(emoji: "✈️", name: "airplane", keywords: "flight travel plane"),
        Entry(emoji: "🚗", name: "car", keywords: "drive automobile"),
        Entry(emoji: "🚂", name: "locomotive", keywords: "train railway"),
        Entry(emoji: "🚲", name: "bicycle", keywords: "bike cycle"),
        Entry(emoji: "🏠", name: "house", keywords: "home"),
        Entry(emoji: "🏢", name: "office building", keywords: "work company"),
        Entry(emoji: "🏖️", name: "beach with umbrella", keywords: "vacation holiday"),
        Entry(emoji: "🗺️", name: "world map", keywords: "travel navigation"),
        // Activities
        Entry(emoji: "⚽", name: "soccer ball", keywords: "football sport"),
        Entry(emoji: "🏀", name: "basketball", keywords: "sport"),
        Entry(emoji: "🎾", name: "tennis", keywords: "sport"),
        Entry(emoji: "🎮", name: "video game", keywords: "gaming controller play"),
        Entry(emoji: "🎲", name: "game die", keywords: "dice random luck"),
        Entry(emoji: "🎵", name: "musical note", keywords: "music song"),
        Entry(emoji: "🎸", name: "guitar", keywords: "music rock"),
        Entry(emoji: "🎤", name: "microphone", keywords: "sing karaoke"),
        Entry(emoji: "📷", name: "camera", keywords: "photo picture"),
        Entry(emoji: "🎬", name: "clapper board", keywords: "movie film"),
    ]
}
