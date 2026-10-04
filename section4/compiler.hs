-- main executable for the toy c compiler
module Main where 

-- standard library imports used by the driver and lexer
import Data.Char (isAlpha, isAlphaNum, isDigit, isSpace, ord)
import Data.List (isPrefixOf, find, partition, intercalate)
import Data.IORef
import System.Environment (getArgs)
import System.IO (hPutStrLn, stderr)
import System.Exit (exitFailure)

-- type covers the c primitive types and nests tptr for pointers
data Type = TInt | TChar | TVoid | TPtr Type 
  deriving(Show, Eq)

-- bop enumerates binary operators our parser understands
data BOp = Add | Sub | Mul | Div | Mod 
          | Eq | Ne | Lt | Le | Gt | Ge 
          | And | Or 

-- uop stores unary operators such as negate and logical not
data UOp = Neg | Not 
  deriving(Show) 

-- expr models all expression forms in our toy c language
data Expr 
  = IntLit Int 
  | CharLit Char 
  | StrLit  String 
  | Var     String 
  | Assign  Expr Expr
  | BinOp   BOp Expr Expr 
  | UnOp    UOp Expr 
  | Call    String [Expr]
  | Index   Expr Expr    -- a[i] == *(a + i)
  | Deref   Expr         -- *p 
  | AddrOf  Expr          -- &x 
  deriving(Show)

-- stmt captures statement forms like blocks, conditionals, loops, and declarations
data Stmt 
  = SExpr    Expr 
  | SBlock   [Stmt]
  | SIf      Expr Stmt (Maybe Stmt)
  | SWhile   Expr Stmt 
  | SReturn  (Maybe Expr)
  | SDecl    Type String (Maybe Expr)
  deriving(Show)

-- topdecl distinguishes function definitions from global variables
data TopDecl 
  = Func { fRet :: Type, fName :: String
          , fParams :: [(Type, String)], fBody :: Stmt }
    | Global { gType :: Type, gName:: String, gInit:: Maybe Expr }
    deriving(Show)

-- lexer 
-- tok variants correspond to the tokens produced by the lexer
data Tok 
  = TKw String 
  | TId String 
  | TIntL Int 
  | TChrL Char 
  | TStrL String 
  | TP    String 
  | TO    String 
  deriving(Show, Eq)

-- keywords marks reserved words so identifiers do not capture them
keywords :: [String]
keywords = ["int", "char", "void", "if", "while", "return"]

-- operators is ordered so multi-character symbols match before single characters
operators :: [String]
operators =
    [ "==","!=","<=",">=","&&","||"
    , "+","-","*","/","%","<",">","=","!","&"
    ]

-- tokenize consumes the input characters and returns a token list
tokenize :: String -> [Tok]
tokenize [] = []
tokenize ('/':'/':cs) = tokenize (dropLine cs)
tokenize ('/':'*':cs) = tokenize (skipBlock cs)
tokenize (c:cs)
  | isSpace c = tokenize cs
  | isDigit c = let (digits, rest) = span isDigit (c:cs)
                in TIntL (read digits) : tokenize rest
  | c == '\'' = let (ch, rest) = readChar cs in TChrL ch : tokenize rest
  | c == '"'  = let (str, rest) = readStr cs in TStrL str : tokenize rest
  | isAlpha c || c == '_' =
      let (word, rest) = span isIdentChar (c:cs)
      in (if word `elem` keywords then TKw word else TId word) : tokenize rest
  | c `elem` "(){}[];," = TP [c] : tokenize cs
  | otherwise =
      case find (`isPrefixOf` (c:cs)) operators of
        Just op -> TO op : tokenize (drop (length op) (c:cs))
        Nothing -> error ("lex: unexpected char " ++ show c)

-- dropline skips characters until the next newline so // comments vanish
dropLine :: String -> String
dropLine [] = []
dropLine ('\n':rest) = rest
dropLine (_:rest) = dropLine rest

-- skipblock consumes everything up to the closing */ or reports an error
skipBlock :: String -> String
skipBlock [] = error "unterminated /* comment"
skipBlock ('*':'/':rest) = rest
skipBlock (_:rest) = skipBlock rest

-- isidentchar accepts letters, digits, and underscores for identifiers
isIdentChar :: Char -> Bool
isIdentChar ch = isAlphaNum ch || ch == '_'

-- readchar parses a character literal and handles escapes
readChar :: String -> (Char, String)
readChar ('\\':e:'\'':rest) = (esc e, rest)
readChar (ch:'\'':rest) = (ch, rest)
readChar _ = error "lex: malformed character literal"

-- readstr parses a string literal and delegates escapes to esc
readStr :: String -> (String, String)
readStr = go ""
  where
    go acc ('"':r) = (reverse acc, r)
    go acc ('\\':e:r) = go (esc e : acc) r
    go acc (ch:r) = go (ch : acc) r
    go _ [] = error "unterminated string"

-- esc converts escape codes into actual characters
esc :: Char -> Char 
esc 'n' = '\n'
esc 't' = '\t'
esc '0' = '\0'
esc '\\' = '\\'
esc '\'' = '\''
esc '"' = '"'
esc c = c

expect :: Tok -> [Tok] -> [Tok]
expect t (t':rest)
  | t == t' = rest 
expect t rest = error ("parse: expected " ++ show t ++ ", got " ++ preview rest)

preview :: [Tok] -> String 
preview ts = show (take 3 ts)

parseProgram :: [Tok] -> [TopDecl]
parseProgram [] = [] 
parseProgram ts =
  let (decl, rest) = parseTop ts 
  in decl: parseProgram rest

parseType :: [Tok] -> (Type, [Tok])
parseType (TKw "int" : rest) = parsePtr TInt rest 
parseType (TKw "char" : rest) = parsePtr TChar rest 
parseType (TKw "void" : rest) = parsePtr TVoid rest 
parseType ts = error("parse: expected type, got " ++ preview ts)

-- parseptr consumes trailing stars, nesting tptr for each pointer level
parsePtr :: Type -> [Tok] -> (Type, [Tok])
parsePtr t (TO "*" : rest) = parsePtr (TPtr t) rest
parsePtr t rest = (t, rest)

-- parsetop reads one top-level declaration: a function or a global variable.
-- both start with a type and a name; a '(' after the name means it is a function.
parseTop :: [Tok] -> (TopDecl, [Tok])
parseTop ts =
  let (ty, r1) = parseType ts
  in case r1 of
       (TId name : TP "(" : r2) ->
         let (params, r3) = parseParams r2
             (body, r4) = parseBlock r3
         in (Func ty name params body, r4)
       (TId name : rest) -> parseGlobal ty name rest
       _ -> error ("parse: expected declarator, got " ++ preview r1)

-- parseglobal finishes a global variable: optional initializer, then ';'
parseGlobal :: Type -> String -> [Tok] -> (TopDecl, [Tok])
parseGlobal ty name (TO "=" : rest) =
  let (e, r) = parseExpr rest
  in (Global ty name (Just e), expect (TP ";") r)
parseGlobal ty name rest =
  (Global ty name Nothing, expect (TP ";") rest)

-- parseparams reads a comma-separated parameter list up to the closing ')'
parseParams :: [Tok] -> ([(Type, String)], [Tok])
parseParams (TP ")" : rest) = ([], rest)
parseParams (TKw "void" : TP ")" : rest) = ([], rest)
parseParams ts =
  let (ty, r1) = parseType ts
  in case r1 of
       (TId name : TP "," : r2) ->
         let (ps, r3) = parseParams r2 in ((ty, name) : ps, r3)
       (TId name : TP ")" : r2) -> ([(ty, name)], r2)
       _ -> error ("parse: bad parameter, got " ++ preview r1)

