-- Устный калькулятор: фраза целиком в jarvis.context.phrase (после эвристики mental_math).

local phrase = (jarvis.context.phrase or ""):lower()
local lang = jarvis.context.language or "ru"

local ru_units = {
    ["ноль"] = 0, ["один"] = 1, ["одна"] = 1, ["два"] = 2, ["две"] = 2, ["три"] = 3,
    ["четыре"] = 4, ["пять"] = 5, ["шесть"] = 6, ["семь"] = 7, ["восемь"] = 8, ["девять"] = 9,
}
local ru_teens = {
    ["десять"] = 10, ["одиннадцать"] = 11, ["двенадцать"] = 12, ["тринадцать"] = 13,
    ["четырнадцать"] = 14, ["пятнадцать"] = 15, ["шестнадцать"] = 16, ["семнадцать"] = 17,
    ["восемнадцать"] = 18, ["девятнадцать"] = 19,
}
local ru_tens = {
    ["двадцать"] = 20, ["тридцать"] = 30, ["сорок"] = 40, ["пятьдесят"] = 50,
    ["шестьдесят"] = 60, ["семьдесят"] = 70, ["восемьдесят"] = 80, ["девяносто"] = 90,
}
local ru_hundreds = {
    ["сто"] = 100, ["двести"] = 200, ["триста"] = 300, ["четыреста"] = 400,
    ["пятьсот"] = 500, ["шестьсот"] = 600, ["семьсот"] = 700, ["восемьсот"] = 800, ["девятьсот"] = 900,
}

local en_units = {
    ["zero"] = 0, ["one"] = 1, ["two"] = 2, ["three"] = 3, ["four"] = 4, ["five"] = 5,
    ["six"] = 6, ["seven"] = 7, ["eight"] = 8, ["nine"] = 9, ["ten"] = 10,
    ["eleven"] = 11, ["twelve"] = 12, ["thirteen"] = 13, ["fourteen"] = 14, ["fifteen"] = 15,
    ["sixteen"] = 16, ["seventeen"] = 17, ["eighteen"] = 18, ["nineteen"] = 19,
}
local en_tens = {
    ["twenty"] = 20, ["thirty"] = 30, ["forty"] = 40, ["fifty"] = 50,
    ["sixty"] = 60, ["seventy"] = 70, ["eighty"] = 80, ["ninety"] = 90,
}

local function trim_punct(w)
    return (w:gsub("^[%p%s]+", ""):gsub("[%p%s]+$", ""))
end

local function split_words(s)
    local t = {}
    for w in s:gmatch("%S+") do
        table.insert(t, trim_punct(w))
    end
    return t
end

--- Разбор целого 0..999 из последовательности слов (ru), жадно с позиции i.
local function parse_ru_group(words, i)
    local total = 0
    local n = #words
    if i > n then return nil, i end

    if ru_hundreds[words[i]] then
        total = total + ru_hundreds[words[i]]
        i = i + 1
    end
    if i <= n and ru_tens[words[i]] then
        total = total + ru_tens[words[i]]
        i = i + 1
        if i <= n and ru_units[words[i]] and ru_units[words[i]] < 10 then
            total = total + ru_units[words[i]]
            i = i + 1
        end
        return total, i
    end
    if i <= n and ru_teens[words[i]] then
        total = total + ru_teens[words[i]]
        return total, i + 1
    end
    if i <= n and ru_units[words[i]] ~= nil then
        return total + ru_units[words[i]], i + 1
    end
    if total > 0 then return total, i end
    return nil, i
end

local function parse_ru_number(words, i)
    local n = #words
    if i > n then return nil, i end
    local v, j = parse_ru_group(words, i)
    if v == nil then return nil, i end
    if j <= n and (words[j] == "тысяча" or words[j] == "тысячи" or words[j] == "тысяч") then
        v = v * 1000
        j = j + 1
        local rest, k = parse_ru_group(words, j)
        if rest then v = v + rest; j = k end
    end
    return v, j
end

local function parse_en_number(words, i)
    local n = #words
    if i > n then return nil, i end
    local w = words[i]
    if en_units[w] then
        return en_units[w], i + 1
    end
    if en_tens[w] then
        local t = en_tens[w]
        if i + 1 <= n and en_units[words[i + 1]] and en_units[words[i + 1]] < 10 then
            return t + en_units[words[i + 1]], i + 2
        end
        return t, i + 1
    end
    return nil, i
end

local function strip_noise(words, lang)
    local noise = {
        ["сколько"] = true, ["будет"] = true, ["посчитай"] = true, ["посчитайте"] = true,
        ["вычисли"] = true, ["реши"] = true, ["калькулятор"] = true,
        ["пример"] = true, ["примеры"] = true, ["задача"] = true, ["задачу"] = true,
        ["найди"] = true, ["дай"] = true, ["скажи"] = true, ["джарвис"] = true, ["jarvis"] = true,
        ["what"] = true, ["is"] = true, ["the"] = true, ["a"] = true, ["calculate"] = true,
        ["how"] = true, ["much"] = true,
    }
    local out = {}
    for _, w in ipairs(words) do
        if not noise[w] then table.insert(out, w) end
    end
    return out
end

local function replace_ops_token(w, lang)
    if lang == "ru" then
        if w == "плюс" then return "+" end
        if w == "минус" then return "-" end
        if w == "умножить" or w == "умножь" then return "*" end
        if w == "разделить" or w == "поделить" then return "/" end
        if w == "на" then return nil end -- часть «разделить на»
    else
        if w == "plus" then return "+" end
        if w == "minus" then return "-" end
        if w == "times" or w == "multiplied" then return "*" end
        if w == "divide" or w == "divided" or w == "over" then return "/" end
        if w == "by" then return nil end
    end
    return nil
end

--- Есть ли с позиции i начало выражения (число, цифры или унарный минус перед числом).
local function can_start_expression(words, i, lang)
    local n = #words
    if i > n then
        return false
    end
    local w = words[i]
    if w:match("^%d+$") then
        return true
    end
    if lang == "ru" then
        if select(1, parse_ru_number(words, i)) then
            return true
        end
    else
        if select(1, parse_en_number(words, i)) then
            return true
        end
    end
    if replace_ops_token(w, lang) == "-" and i + 1 <= n then
        if lang == "ru" then
            return select(1, parse_ru_number(words, i + 1)) ~= nil
        else
            return select(1, parse_en_number(words, i + 1)) ~= nil
        end
    end
    return false
end

--- Срезать опечатки/мусор ASR в начале («ненависть реши пример пять плюс пять» → с «пять»).
local function drop_leading_junk(words, lang)
    local i = 1
    while i <= #words and not can_start_expression(words, i, lang) do
        i = i + 1
    end
    if i > #words then
        return {}
    end
    local out = {}
    for j = i, #words do
        table.insert(out, words[j])
    end
    return out
end

local function words_to_tokens(words, lang)
    local tokens = {}
    local i = 1
    while i <= #words do
        local w = words[i]
        local op = replace_ops_token(w, lang)
        if op then
            table.insert(tokens, op)
            i = i + 1
        elseif w:match("^%d+$") then
            table.insert(tokens, tonumber(w))
            i = i + 1
        else
            local num, ni
            if lang == "ru" then
                num, ni = parse_ru_number(words, i)
            else
                num, ni = parse_en_number(words, i)
            end
            if num then
                table.insert(tokens, num)
                i = ni
            else
                return nil, "не удалось разобрать число: " .. w
            end
        end
    end
    return tokens, nil
end

local function eval_tokens(tokens)
    if #tokens == 0 then return nil, "пусто" end
    if type(tokens[1]) ~= "number" then return nil, "ожидалось число" end
    local acc = tokens[1]
    local k = 2
    while k <= #tokens do
        if type(tokens[k]) ~= "string" or type(tokens[k + 1]) ~= "number" then
            return nil, "ожидалось операция и число"
        end
        local op = tokens[k]
        local rhs = tokens[k + 1]
        if op == "+" then acc = acc + rhs
        elseif op == "-" then acc = acc - rhs
        elseif op == "*" then acc = acc * rhs
        elseif op == "/" then
            if rhs == 0 then return nil, "деление на ноль" end
            acc = math.floor(acc / rhs)
        else return nil, "неизвестная операция" end
        k = k + 2
    end
    return acc, nil
end

local words = split_words(phrase)
words = strip_noise(words, lang)
words = drop_leading_junk(words, lang)

-- «разделить на X» / «divided by X»
local flat = {}
local j = 1
while j <= #words do
    if lang == "ru" and words[j] == "разделить" and words[j + 1] == "на" then
        table.insert(flat, "/")
        j = j + 2
    elseif lang == "ru" and words[j] == "поделить" and words[j + 1] == "на" then
        table.insert(flat, "/")
        j = j + 2
    elseif lang == "ru" and words[j] == "умножить" and words[j + 1] == "на" then
        table.insert(flat, "*")
        j = j + 2
    elseif lang ~= "ru" and words[j] == "divided" and words[j + 1] == "by" then
        table.insert(flat, "/")
        j = j + 2
    elseif lang ~= "ru" and words[j] == "multiplied" and words[j + 1] == "by" then
        table.insert(flat, "*")
        j = j + 2
    else
        table.insert(flat, words[j])
        j = j + 1
    end
end
words = flat

local tokens, err = words_to_tokens(words, lang)
if not tokens then
    jarvis.log("warn", err or "parse")
    jarvis.system.notify("Jarvis", err or "Ошибка разбора")
    jarvis.audio.play_error()
    return { chain = false }
end

local result, e2 = eval_tokens(tokens)
if not result then
    jarvis.log("warn", e2 or "eval")
    jarvis.system.notify("Jarvis", e2 or "Ошибка")
    jarvis.audio.play_error()
    return { chain = false }
end

local msg
if lang == "ru" then
    msg = "Результат: " .. tostring(result)
else
    msg = "Result: " .. tostring(result)
end

jarvis.log("info", msg)
jarvis.speak(msg)
jarvis.system.notify("Jarvis", tostring(result))

return { chain = true }
