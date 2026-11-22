# Prompt : page « Activity Tracker » pour le site ef tools

Copie tout le bloc ci-dessous dans ton outil (Claude, Lovable, Framer AI, v0…) avec les images du dossier `docs/press/`.

---

Crée une page dédiée à mon logiciel **Activity Tracker** sur mon site personnel **ef tools** (par Enzo Freminet). La page doit reprendre exactement le style existant du site : fond sombre, typographie **Manrope**, esthétique minimaliste et premium inspirée des pages produit d'Apple. Beaucoup d'espace, de grands titres serrés (graisse 800, interlettrage négatif), des textes secondaires gris (#a1a1a6), des cartes gris très foncé (#1c1c1e) aux coins très arrondis (24 px) sans bordure. Une seule couleur d'accent : bleu #0a84ff, utilisée avec parcimonie. Pas de dégradés voyants, pas de néons, pas d'emoji dans les titres.

## Animations
Fluides et discrètes, au défilement :
- éléments qui apparaissent en montant légèrement avec un fondu (translateY 24 px → 0, flou 6 px → 0, 700 ms, easing cubic-bezier(.22,1,.36,1)), en cascade de 80 ms ;
- la capture principale grossit très légèrement en arrivant dans l'écran (scale .96 → 1) ;
- les chiffres clés comptent jusqu'à leur valeur ;
- boutons en forme de pilule qui se compriment légèrement au clic.

Respecte `prefers-reduced-motion` (aucune animation). Page responsive, parfaite sur mobile (marges de 16 px, aucun défilement horizontal).

## Images fournies
- `01-hero.png` : visuel principal (16:9)
- `02-fonctionnalites.png` : grille des fonctionnalités
- `03-leger-prive.png` : chiffres clés
- `capture-today.png`, `capture-week.png`, `capture-wrapped.png`, `capture-year.png`, `capture-apps.png` : captures de l'interface (2880×1800)
- `wrapped-partage.png` : image de partage du Wrapped (format portrait 4:5)
- `og-partage-1200x630.png` : à utiliser comme image Open Graph / Twitter de la page

Affiche les captures dans un cadre de fenêtre sobre (barre de titre gris foncé avec trois points gris, coins arrondis, ombre très douce). Mets `loading="lazy"` sauf sur l'image du haut, et un texte alternatif descriptif pour chaque image.

## Structure et textes (en français, tutoiement)

**1. Hero**
- Petit sur-titre : « Activity Tracker · pour Windows 10/11 »
- Titre : « Ton temps d'écran, sans le bruit. »
- Sous-titre : « Un traceur d'activité léger et privé. Pensé pour les artistes 3D, utile à tout le monde. »
- Bouton principal blanc : « Télécharger gratuitement » → `https://github.com/bruce333-creator/Activity_Watch/releases/latest/download/activity-tracker.exe`
- Lien secondaire : « Voir le code source » → `https://github.com/bruce333-creator/Activity_Watch`
- Sous le bouton, en petit gris : « Version gratuite · ~2 Mo · Windows 10 et 11 »
- Puis la capture `capture-today.png` en grand, dans le cadre de fenêtre.

**2. « Tout ce qui compte. Rien de plus. »** : grille de 6 cartes :
- **Temps d'écran réel** : « Seul le temps où tu utilises vraiment ton PC est compté. Pauses, veille et écran verrouillé sont ignorés. »
- **Wrapped chaque semaine** : « Ton profil de la semaine (Marathonien, Oiseau de nuit, Zen…), ton top 5 et tes records, à partager en une image. »
- **Temps par projet** : « Maya, Blender, Houdini, Photoshop… Le temps passé sur chaque scène est reconnu automatiquement. »
- **Bien-être** : « Rappels de pause, objectif quotidien et limites par appli, en notifications Windows. »
- **Ton année** : « Un calendrier où chaque jour est une case, avec tes séries créatives et tes records. »
- **À ton image** : « Renomme tes applis et change leurs logos. Export CSV inclus. »

**3. Section Wrapped** : texte à gauche, `wrapped-partage.png` à droite, légèrement incliné (rotate -3°), qui se redresse au survol.
- Titre : « Ta semaine, racontée. »
- Texte : « Chaque lundi, Activity Tracker te prépare ton Wrapped : combien de temps tu as passé sur ton PC, ton appli n°1, ton jour le plus chargé et le profil qui te ressemble. »

**4. Galerie** : carrousel horizontal ou onglets « Aujourd'hui / Semaine / Année / Applis » qui changent la capture avec un fondu enchaîné.

**5. « Léger. Privé. Toujours à jour. »** : 4 chiffres clés qui comptent à l'apparition :
- « ~2 Mo » : un seul .exe, sans installation ni dépendance
- « 0 » : télémétrie. Tes données restent sur ton PC
- « 2 s » : un coup d'œil toutes les 2 secondes, puis il se rendort
- « 1 clic » : pour installer chaque nouvelle version

**6. Installation en 3 étapes** (numéros 1-2-3 dans des cercles) :
1. Télécharge `activity-tracker.exe` et range-le dans un dossier à toi, par exemple Documents\ActivityTracker.
2. Double-clique dessus : l'icône apparaît près de l'horloge de Windows.
3. Clic droit sur l'icône → « Lancer au démarrage de Windows ». C'est tout.

**7. FAQ** : accordéon animé (hauteur fluide, chevron qui tourne) :
- *Mes données sont-elles envoyées quelque part ?* « Non. Tout est enregistré sur ton PC. La seule connexion est une vérification de mise à jour une fois par jour auprès de GitHub, sans aucune donnée personnelle. Elle se désactive dans les réglages. »
- *Est-ce que ça ralentit mon PC ?* « Non. Il regarde l'appli au premier plan toutes les 2 secondes puis se rendort, en priorité basse. Il n'enregistre ni les touches ni l'écran. »
- *Windows affiche « Windows a protégé votre ordinateur ».* « C'est normal pour un logiciel récent. Clique sur « Informations complémentaires » puis « Exécuter quand même ». Les versions signées font disparaître ce message. »
- *Comment mettre à jour ?* « Une notification te prévient quand une nouvelle version sort : un clic, et elle s'installe toute seule sans perdre tes données. »
- *Mac ou Linux ?* « Pour l'instant, Windows 10 et 11 uniquement. »
- *C'est vraiment gratuit ?* « Oui, et open source (licence MIT). »

**8. Dernier appel** : grand titre centré « Prêt à voir où passe ton temps ? » + bouton « Télécharger gratuitement » (même lien).

**9. Pied de page**, en petit gris :
- « Activity Tracker par Enzo Freminet · ef tools · Licence MIT »
- (une fois la signature SignPath active) « Signature de code gratuite fournie par SignPath.io, certificat par SignPath Foundation », avec un lien vers https://github.com/bruce333-creator/Activity_Watch#code-signing-policy

## Technique
- Balises meta : titre « Activity Tracker : ton temps d'écran sur Windows | ef tools », description de 155 caractères maximum, Open Graph avec `og-partage-1200x630.png`.
- Pas de bibliothèque d'animation lourde : IntersectionObserver + CSS suffisent.
- Contraste accessible, focus clavier visible.
- Ne change rien au reste du site : ajoute seulement cette page et un lien « Activity Tracker » dans le menu, à côté de mes autres outils.
