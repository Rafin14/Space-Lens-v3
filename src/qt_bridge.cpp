// Thin Qt ABI adapter. Application state, filesystem work and drawing decisions live in Rust.

#include <QtWidgets/QtWidgets>

#include <QtGui/QSurfaceFormat>

#include <QtCore/QElapsedTimer>

#include <cstdint>

#include <cstring>

#include <atomic>



extern "C" {

void rust_event(int id, int64_t a, int64_t b, double x, double y, double delta, const char* text);

void rust_tick();

void rust_input();

void rust_paint(int canvas, void* painter, int width, int height, double ratio);

char* rust_model_data(int model, uint64_t node, int row, int column, int role);

void rust_string_free(char* text);

int rust_model_count(int model, uint64_t parent);

uint64_t rust_model_child(uint64_t parent, int row);

uint64_t rust_model_parent(uint64_t node, int* row);

int rust_can_close(int kind);

}



static QApplication* application=nullptr;

static QTimer* pollTimer=nullptr;

static std::atomic_bool tickQueued=false;

static QPalette pendingPalette;

static bool paletteBatch=false;

static void scheduleTick(){if(!application||tickQueued.exchange(true))return;QMetaObject::invokeMethod(application,[]{tickQueued=false;rust_tick();},Qt::QueuedConnection);}

struct TextRun {QString name,value,elidedName,elidedValue;QFont font;double width=0;int nameLimit=-1,valueLimit=-1;};




static QString text(const char* value){return QString::fromUtf8(value?value:"");}

static void send_event(int id,int64_t a=0,int64_t b=0,double x=0,double y=0,double delta=0,const QString& value={}){

    auto utf8=value.toUtf8();rust_event(id,a,b,x,y,delta,utf8.constData());scheduleTick();

}

static void send_input(int id,int64_t a=0,int64_t b=0,double x=0,double y=0,double delta=0){send_event(id,a,b,x,y,delta);rust_input();}

class Window final:public QMainWindow {

    void closeEvent(QCloseEvent* e)override{if(!rust_can_close(1)){e->ignore();return;}send_event(999);QMainWindow::closeEvent(e);}

    void resizeEvent(QResizeEvent* e)override{QMainWindow::resizeEvent(e);send_event(998,e->size().width(),e->size().height());}

};

class Dialog final:public QDialog {

    void showEvent(QShowEvent* e)override{setProperty("sl_initial_opacity",windowOpacity());QDialog::showEvent(e);}

public:

    Dialog(QWidget* parent):QDialog(parent){}

    void reject()override{if(property("sl_map").toBool()){if(rust_can_close(2))send_event(76);return;}QDialog::reject();}

    void closeEvent(QCloseEvent* e)override{if(property("sl_map").toBool()&&!rust_can_close(2)){e->ignore();return;}QDialog::closeEvent(e);}

};

class PathHeading final:public QLabel {

    QString path;

    static constexpr int flags=Qt::AlignLeft|Qt::AlignTop|Qt::TextWrapAnywhere;

    void paintEvent(QPaintEvent*)override{QPainter painter(this);painter.setPen(palette().color(QPalette::WindowText));painter.drawText(contentsRect(),flags,path);}

    void keyPressEvent(QKeyEvent* e)override{if(e->matches(QKeySequence::Copy)){QApplication::clipboard()->setText(path);e->accept();}else QLabel::keyPressEvent(e);}

    void contextMenuEvent(QContextMenuEvent* e)override{QMenu menu(this);auto copy=menu.addAction("Copy path");if(menu.exec(e->globalPos())==copy)QApplication::clipboard()->setText(path);}

public:

    PathHeading(QWidget* parent):QLabel(parent){setTextFormat(Qt::PlainText);setWordWrap(true);setFocusPolicy(Qt::StrongFocus);auto policy=QSizePolicy(QSizePolicy::Ignored,QSizePolicy::Preferred);policy.setHeightForWidth(true);setSizePolicy(policy);}

    QString fullPath()const{return path;}

    int heightForWidth(int width)const override{return fontMetrics().boundingRect(QRect(0,0,qMax(1,width),20000),flags,path).height();}

    QSize sizeHint()const override{return QSize(200,heightForWidth(qMax(1,width())));}

    QSize minimumSizeHint()const override{return QSize(0,fontMetrics().height());}

    void setPath(const QString& value){if(path==value)return;path=value;setToolTip(value);setAccessibleName("Current folder: "+value);QLabel::setText(value);updateGeometry();update();}

};

class Canvas final:public QWidget {

    int id;

public:

    Canvas(QWidget* parent,int canvas):QWidget(parent),id(canvas){setAttribute(Qt::WA_OpaquePaintEvent);setFocusPolicy(canvas<2?Qt::StrongFocus:Qt::NoFocus);setMouseTracking(true);}

    void paintEvent(QPaintEvent*)override{QPainter painter(this);if(id==2){auto font=painter.font();font.setFeature(QFont::Tag("tnum"),1);painter.setFont(font);}rust_paint(id,&painter,width(),height(),devicePixelRatioF());}

    void mousePressEvent(QMouseEvent* e)override{setFocus(Qt::MouseFocusReason);send_input(100+id,0,e->button(),e->position().x(),e->position().y());}

    void mouseReleaseEvent(QMouseEvent* e)override{send_input(100+id,1,e->button(),e->position().x(),e->position().y());}

    void mouseMoveEvent(QMouseEvent* e)override{send_input(100+id,2,e->buttons().toInt(),e->position().x(),e->position().y());}

    void wheelEvent(QWheelEvent* e)override{if(id>=2){QWidget::wheelEvent(e);return;}double delta=e->angleDelta().y()?e->angleDelta().y():e->pixelDelta().y()*3;send_input(100+id,3,0,e->position().x(),e->position().y(),delta);e->accept();}

    void resizeEvent(QResizeEvent* e)override{send_event(100+id,4,0,e->size().width(),e->size().height());}

    void keyPressEvent(QKeyEvent* e)override{if(e->key()==Qt::Key_Plus||e->key()==Qt::Key_Equal||e->key()==Qt::Key_Minus||e->key()==Qt::Key_0){send_input(100+id,5,e->key());e->accept();}else QWidget::keyPressEvent(e);}

    void leaveEvent(QEvent*)override{send_event(100+id,6);}

};

class Model final:public QAbstractItemModel {

    int id;

    QIcon folderIcon,fileIcon;

public:

    Model(QObject* parent,int model):QAbstractItemModel(parent),id(model),

        folderIcon(QApplication::style()->standardIcon(QStyle::SP_DirIcon)),

        fileIcon(QApplication::style()->standardIcon(QStyle::SP_FileIcon)){}

    QModelIndex index(int row,int col,const QModelIndex& parent={})const override{

        if(row<0||col<0||col>=columnCount(parent)||row>=rowCount(parent))return{};

        return createIndex(row,col,id==0?rust_model_child(parent.isValid()?parent.internalId():0,row):uint64_t(row+1));

    }

    QModelIndex parent(const QModelIndex& index)const override{

        if(id!=0||!index.isValid())return{};

        int row=0;auto node=rust_model_parent(index.internalId(),&row);

        return node?createIndex(row,0,node):QModelIndex();

    }

    QModelIndex forNode(uint64_t node)const{int row=0;rust_model_parent(node,&row);return node?createIndex(row,0,node):QModelIndex();}

    int rowCount(const QModelIndex& parent={})const override{return parent.column()>0?0:rust_model_count(id,parent.isValid()?parent.internalId():0);}

    int columnCount(const QModelIndex& ={})const override{return id==0?2:4;}

    QVariant data(const QModelIndex& index,int role=Qt::DisplayRole)const override{

        if(!index.isValid())return{};

        if(role==Qt::TextAlignmentRole)return int(((index.column()==1||index.column()==2)?Qt::AlignRight:Qt::AlignLeft)|Qt::AlignVCenter);

        if(role==Qt::ForegroundRole&&id!=0&&index.column()==3){auto value=rust_model_data(id,index.internalId(),index.row(),index.column(),101);bool partial=value&&value[0]=='1';rust_string_free(value);bool dark=application&&application->property("darkMode").toBool();return QColor(partial?(dark?"#edc887":"#92400e"):(dark?"#acb8c6":"#5d5f60"));}

        if(role==Qt::DecorationRole&&index.column()==0&&id!=0){

            auto value=rust_model_data(id,index.internalId(),index.row(),index.column(),100);

            bool folder=value&&value[0]=='1';rust_string_free(value);

            return folder?folderIcon:fileIcon;

        }

        if(role!=Qt::DisplayRole&&role!=Qt::ToolTipRole&&role!=Qt::AccessibleTextRole&&role!=Qt::AccessibleDescriptionRole)return{};

        auto value=rust_model_data(id,index.internalId(),index.row(),index.column(),role);

        QString result=text(value);rust_string_free(value);return result;

    }

    QVariant headerData(int column,Qt::Orientation orientation,int role)const override{

        if(orientation!=Qt::Horizontal)return{};

        if(role==Qt::TextAlignmentRole)return int(((column==1||column==2)?Qt::AlignRight:Qt::AlignLeft)|Qt::AlignVCenter);

        if(role!=Qt::DisplayRole)return{};

        static const char* table[]={"Name","Logical size","Of folder","Measurement"};

        static const char* tree[]={"Folder","Logical size"};return text(id==0?tree[column]:table[column]);

    }

    Qt::ItemFlags flags(const QModelIndex& index)const override{return index.isValid()?Qt::ItemIsEnabled|Qt::ItemIsSelectable:Qt::NoItemFlags;}

    void reset(){beginResetModel();endResetModel();}

};



extern "C" {

void sl_init(){

    qputenv("QT_WIDGETS_RHI","1");qputenv("QT_WIDGETS_RHI_BACKEND","d3d11");

    QSurfaceFormat format=QSurfaceFormat::defaultFormat();format.setSwapInterval(1);QSurfaceFormat::setDefaultFormat(format);

    static int argc=1;static char name[]="SpaceLens_Rust";static char* argv[]={name,nullptr};

    application=new QApplication(argc,argv);application->setApplicationName("Space Lens");application->setApplicationVersion("3.6.1");application->setOrganizationName("Space Lens Rust");application->setStyle("Fusion");

}

int sl_exec(){return application->exec();}


void sl_quit(){application->quit();}

void sl_timer_mode(int milliseconds){if(!application)return;if(!pollTimer){pollTimer=new QTimer(application);pollTimer->setTimerType(Qt::PreciseTimer);QObject::connect(pollTimer,&QTimer::timeout,[]{rust_tick();});}if(milliseconds<=0){pollTimer->stop();}else if(!pollTimer->isActive()||pollTimer->interval()!=milliseconds){pollTimer->start(milliseconds);}}

void sl_timer(){sl_timer_mode(0);scheduleTick();}

void sl_wake(){scheduleTick();}



void sl_style(const char* css,double scale){QFont font("Segoe UI");font.setPointSizeF(10*scale);application->setFont(font);application->setStyleSheet(text(css));}

void sl_app_dark(int dark){application->setProperty("darkMode",bool(dark));}

void sl_local_time(uint64_t seconds,char* buffer){auto value=QDateTime::fromSecsSinceEpoch(qint64(seconds)).toLocalTime().toString("HH:mm:ss").toUtf8();memcpy(buffer,value.constData(),9);}


void sl_palette_begin(){pendingPalette=application->palette();paletteBatch=true;}

void sl_palette_end(){if(paletteBatch){paletteBatch=false;application->setPalette(pendingPalette);}}

void sl_palette(int role,int group,const char* color){if(paletteBatch){pendingPalette.setColor(QPalette::ColorGroup(group),QPalette::ColorRole(role),QColor(text(color)));}else{auto palette=application->palette();palette.setColor(QPalette::ColorGroup(group),QPalette::ColorRole(role),QColor(text(color)));application->setPalette(palette);}}



void sl_icon(const unsigned char* bytes,int size){QPixmap image;image.loadFromData(bytes,size);QIcon icon(image);application->setWindowIcon(icon);}

void* sl_new(int kind,void* parent,const char* name,int id){

    auto p=static_cast<QWidget*>(parent);QWidget* widget=nullptr;

    switch(kind){

    case 0:widget=new QWidget(p);break;

    case 1:{auto label=new QLabel(p);label->setTextFormat(Qt::PlainText);widget=label;break;}

    case 2:{auto button=new QPushButton(p);QObject::connect(button,&QPushButton::clicked,[id](bool checked){send_event(id,checked);});widget=button;break;}

    case 3:widget=new Window();break;

    case 4:{auto dialog=new Dialog(p);QObject::connect(dialog,&QDialog::finished,[id](int value){send_event(id,value);});widget=dialog;break;}

    case 5:widget=new Canvas(p,id);break;

    case 6:widget=new QTreeView(p);break;

    case 7:widget=new QTableView(p);break;

    case 8:{auto scroll=new QScrollArea(p);scroll->setWidgetResizable(true);scroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);if(text(name)=="rankingArea")scroll->verticalScrollBar()->setObjectName("rankingScroll");widget=scroll;break;}

    case 9:{auto split=new QSplitter(Qt::Horizontal,p);split->setChildrenCollapsible(false);widget=split;break;}

    case 10:widget=new QStackedWidget(p);break;

    case 11:widget=new QTabWidget(p);break;

    case 12:{auto slider=new QSlider(Qt::Horizontal,p);QObject::connect(slider,&QSlider::valueChanged,[id](int value){send_event(id,value);});QObject::connect(slider,&QSlider::sliderReleased,[slider,id]{send_event(id+1,slider->value());});QObject::connect(slider,&QSlider::actionTriggered,[slider,id](int){QTimer::singleShot(0,slider,[slider,id]{if(!slider->isSliderDown())send_event(id+1,slider->value());});});widget=slider;break;}

    case 13:{auto spin=new QSpinBox(p);spin->setKeyboardTracking(false);spin->setButtonSymbols(QAbstractSpinBox::NoButtons);QObject::connect(spin,&QSpinBox::editingFinished,[spin,id]{send_event(id,spin->value());});widget=spin;break;}

    case 14:{auto check=new QCheckBox(p);QObject::connect(check,&QCheckBox::toggled,[id](bool value){send_event(id,value);});widget=check;break;}

    case 15:widget=new QProgressBar(p);break;

    case 16:widget=new PathHeading(p);break;

    }

    widget->setObjectName(text(name));return widget;

}

void sl_destroy(void* widget){static_cast<QWidget*>(widget)->deleteLater();}

void sl_central(void* window,void* child){static_cast<QMainWindow*>(window)->setCentralWidget(static_cast<QWidget*>(child));}

void* sl_layout(int horizontal,void* parent,int margin,int spacing){

    QBoxLayout* layout=horizontal?static_cast<QBoxLayout*>(new QHBoxLayout(static_cast<QWidget*>(parent))):static_cast<QBoxLayout*>(new QVBoxLayout(static_cast<QWidget*>(parent)));

    layout->setContentsMargins(margin,margin,margin,margin);layout->setSpacing(spacing);return layout;

}

void sl_margins(void* layout,int l,int t,int r,int b){static_cast<QLayout*>(layout)->setContentsMargins(l,t,r,b);}

void sl_add(void* layout,void* widget,int stretch){static_cast<QBoxLayout*>(layout)->addWidget(static_cast<QWidget*>(widget),stretch);}

void sl_add_center(void* layout,void* widget){static_cast<QBoxLayout*>(layout)->addWidget(static_cast<QWidget*>(widget),0,Qt::AlignHCenter);}

void sl_add_layout(void* layout,void* child,int stretch){static_cast<QBoxLayout*>(layout)->addLayout(static_cast<QLayout*>(child),stretch);}

void sl_stretch(void* layout,int stretch){static_cast<QBoxLayout*>(layout)->addStretch(stretch);}

void sl_split_add(void* split,void* child){static_cast<QSplitter*>(split)->addWidget(static_cast<QWidget*>(child));}

void sl_split_sizes(void* split,int a,int b){static_cast<QSplitter*>(split)->setSizes({a,b});}

void sl_split_stretch(void* split,int index,int factor){static_cast<QSplitter*>(split)->setStretchFactor(index,factor);}

void sl_fixed(void* widget,int width,int height){static_cast<QWidget*>(widget)->setFixedSize(width,height);}

void sl_scroll_child(void* scroll,void* child){static_cast<QScrollArea*>(scroll)->setWidget(static_cast<QWidget*>(child));}

void sl_tab_add(void* tabs,void* child,const char* title){static_cast<QTabWidget*>(tabs)->addTab(static_cast<QWidget*>(child),text(title));}

void sl_tab_clear(void* tabs){auto tab=static_cast<QTabWidget*>(tabs);while(tab->count())tab->removeTab(0);}

void sl_stack_add(void* stack,void* child){static_cast<QStackedWidget*>(stack)->addWidget(static_cast<QWidget*>(child));}

void sl_stack_set(void* stack,void* child){static_cast<QStackedWidget*>(stack)->setCurrentWidget(static_cast<QWidget*>(child));}

void sl_set(void* object,const char* name,const char* value){

    auto target=static_cast<QObject*>(object);QSignalBlocker block(target);

    if(auto heading=dynamic_cast<PathHeading*>(target);heading&&strcmp(name,"text")==0)heading->setPath(text(value));

    else target->setProperty(name,text(value));

}

void sl_int(void* object,const char* name,int value){auto target=static_cast<QObject*>(object);QSignalBlocker block(target);target->setProperty(name,value);}

void sl_bool(void* object,const char* name,int value){auto target=static_cast<QObject*>(object);QSignalBlocker block(target);target->setProperty(name,bool(value));}

void sl_wrap(void* widget,int wrap){static_cast<QLabel*>(widget)->setWordWrap(wrap);}

void sl_ignored(void* widget){static_cast<QWidget*>(widget)->setSizePolicy(QSizePolicy::Ignored,QSizePolicy::Preferred);}

void sl_align(void* widget,int alignment){static_cast<QLabel*>(widget)->setAlignment(Qt::Alignment(alignment));}

void sl_show(void* widget,int fullscreen){auto w=static_cast<QWidget*>(widget);if(w->isWindow()){for(auto animation:w->findChildren<QPropertyAnimation*>(QString(),Qt::FindDirectChildrenOnly)){if(animation->propertyName()=="windowOpacity"){animation->stop();animation->deleteLater();}}w->setWindowOpacity(1.);}if(fullscreen)w->showFullScreen();else w->show();w->raise();}

void sl_hide(void* widget){static_cast<QWidget*>(widget)->hide();}

void sl_focus(void* widget){static_cast<QWidget*>(widget)->setFocus(Qt::OtherFocusReason);}

void sl_dialog(void* widget,int kind){auto dialog=static_cast<QDialog*>(widget);if(kind==1){dialog->setWindowFlags(Qt::Dialog|Qt::FramelessWindowHint);dialog->setModal(true);dialog->setProperty("sl_map",true);}else if(kind==2){dialog->setWindowFlags(Qt::Popup);}else dialog->setModal(true);}

void sl_popup(void* widget,void* anchor){auto popup=static_cast<QWidget*>(widget);auto button=static_cast<QWidget*>(anchor);popup->adjustSize();auto pos=button->mapToGlobal(QPoint(button->width()-popup->width(),button->height()+8));auto screen=button->screen()->availableGeometry();pos.setX(qBound(screen.left(),pos.x(),screen.right()-popup->width()));pos.setY(qBound(screen.top(),pos.y(),screen.bottom()-popup->height()));popup->move(pos);popup->show();}

void sl_cursor(void* widget,int closed){auto w=static_cast<QWidget*>(widget);if(closed)w->setCursor(Qt::ClosedHandCursor);else w->unsetCursor();}

void sl_brand(void* widget,int size){static_cast<QLabel*>(widget)->setPixmap(application->windowIcon().pixmap(size,size));}

void sl_resize(void* widget,int width,int height){static_cast<QWidget*>(widget)->resize(width,height);}

int sl_width(void* widget){return static_cast<QWidget*>(widget)->width();}

int sl_height(void* widget){return static_cast<QWidget*>(widget)->height();}

int sl_hint_width(void* widget){return static_cast<QWidget*>(widget)->minimumSizeHint().width();}

int sl_is_visible(void* widget){return static_cast<QWidget*>(widget)->isVisible();}

double sl_opacity(void* widget){return static_cast<QWidget*>(widget)->windowOpacity();}

int64_t sl_hwnd(void* widget){return int64_t(static_cast<QWidget*>(widget)->winId());}

void sl_group(void* parent,void** buttons,int count){auto group=new QButtonGroup(static_cast<QObject*>(parent));for(int i=0;i<count;i++)group->addButton(static_cast<QAbstractButton*>(buttons[i]));}

void sl_navigation_icon(void* widget,int direction,double scale){

    QIcon icon;

    for(auto mode:{QIcon::Normal,QIcon::Disabled}){

        QPixmap pixmap(40,40);pixmap.fill(Qt::transparent);QPainter painter(&pixmap);painter.setRenderHint(QPainter::Antialiasing);painter.scale(2,2);

        painter.setPen(QPen(application->palette().color(mode==QIcon::Normal?QPalette::Active:QPalette::Disabled,QPalette::ButtonText),1.8,Qt::SolidLine,Qt::RoundCap,Qt::RoundJoin));

        QPolygon shape=direction==0?QPolygon({{12,5},{7,10},{12,15}}):direction==1?QPolygon({{8,5},{13,10},{8,15}}):QPolygon({{5,12},{10,7},{15,12}});painter.drawPolyline(shape);painter.end();icon.addPixmap(pixmap,mode);

    }

    auto button=static_cast<QPushButton*>(widget);button->setIcon(icon);button->setIconSize(QSize(qRound(16*scale),qRound(16*scale)));

}

void sl_view(void* widget,int id,int event_id,double scale){

    auto view=static_cast<QAbstractItemView*>(widget);auto model=new Model(view,id);view->setModel(model);

    view->setSelectionBehavior(QAbstractItemView::SelectRows);view->setSelectionMode(QAbstractItemView::SingleSelection);view->setEditTriggers(QAbstractItemView::NoEditTriggers);

    QObject::connect(view->selectionModel(),&QItemSelectionModel::currentChanged,[id,event_id](const QModelIndex& index,const QModelIndex&){send_event(event_id,id==0?int64_t(index.internalId()):index.row());});

    QObject::connect(view,&QAbstractItemView::activated,[id,event_id](const QModelIndex& index){send_event(event_id+1,id==0?int64_t(index.internalId()):index.row());});

    if(id==0){auto tree=static_cast<QTreeView*>(view);tree->setUniformRowHeights(true);tree->setIndentation(qRound(16*scale));tree->setAnimated(false);tree->setExpandsOnDoubleClick(false);tree->header()->setStretchLastSection(false);tree->header()->setSectionResizeMode(0,QHeaderView::Stretch);tree->header()->setSectionResizeMode(1,QHeaderView::Fixed);tree->setColumnWidth(1,qRound(98*scale));

        QObject::connect(tree,&QTreeView::expanded,[event_id](const QModelIndex& index){send_event(event_id+2,index.internalId(),1);});

        QObject::connect(tree,&QTreeView::collapsed,[event_id](const QModelIndex& index){send_event(event_id+2,index.internalId(),0);});

    }else{auto table=static_cast<QTableView*>(view);table->setShowGrid(false);table->setWordWrap(false);table->setTextElideMode(Qt::ElideMiddle);table->verticalHeader()->hide();table->verticalHeader()->setDefaultSectionSize(qRound(44*scale));table->horizontalHeader()->setSectionResizeMode(0,QHeaderView::Stretch);table->horizontalHeader()->setStretchLastSection(false);int widths[]={0,110,80,106};for(int i=1;i<4;i++){table->horizontalHeader()->setSectionResizeMode(i,QHeaderView::Fixed);table->setColumnWidth(i,qRound(widths[i]*scale));}table->horizontalHeader()->setMinimumSectionSize(qRound(80*scale));}

}

void sl_model_reset(void* widget){auto view=static_cast<QAbstractItemView*>(widget);static_cast<Model*>(view->model())->reset();}

int sl_row(void* widget){return static_cast<QAbstractItemView*>(widget)->currentIndex().row();}

void sl_select_row(void* widget,int row){auto view=static_cast<QAbstractItemView*>(widget);view->setCurrentIndex(view->model()->index(row,0));view->scrollTo(view->model()->index(row,0));}

void sl_tree_expand(void* widget,uint64_t node,int expanded){auto tree=static_cast<QTreeView*>(widget);tree->setExpanded(static_cast<Model*>(tree->model())->forNode(node),expanded);}

void sl_tree_select(void* widget,uint64_t node){auto tree=static_cast<QTreeView*>(widget);tree->setCurrentIndex(static_cast<Model*>(tree->model())->forNode(node));tree->scrollTo(tree->currentIndex());}

void sl_collapse(void* widget){static_cast<QTreeView*>(widget)->collapseAll();}

void* sl_action(void* widget,const char* name,const char* shortcut,int id,int separator){

    auto w=static_cast<QWidget*>(widget);auto action=new QAction(text(name),w);action->setSeparator(separator);if(shortcut&&*shortcut){action->setShortcut(QKeySequence(text(shortcut)));action->setShortcutContext(Qt::WidgetWithChildrenShortcut);}QObject::connect(action,&QAction::triggered,[id]{send_event(id);});w->addAction(action);w->setContextMenuPolicy(Qt::ActionsContextMenu);return action;

}

void sl_shortcut(void* widget,const char* key,int id){auto w=static_cast<QWidget*>(widget);auto action=new QAction(w);action->setShortcut(QKeySequence(text(key)));QObject::connect(action,&QAction::triggered,[id]{send_event(id);});w->addAction(action);}

char* sl_choose(void* widget){auto folder=QFileDialog::getExistingDirectory(static_cast<QWidget*>(widget),"Choose a folder to inspect",static_cast<QWidget*>(widget)->property("sl_root").toString());auto value=folder.toUtf8();auto result=new char[value.size()+1];memcpy(result,value.constData(),value.size()+1);return result;}

void sl_cpp_free(char* value){delete[] value;}

void sl_clipboard(const char* value){application->clipboard()->setText(text(value));}

void sl_shortcuts(void* widget,double scale){
    auto owner=static_cast<QWidget*>(widget);
    if(auto existing=owner->findChild<QDialog*>("shortcutsDialog",Qt::FindDirectChildrenOnly)){existing->show();existing->raise();existing->activateWindow();return;}
    auto dialog=new QDialog(owner);dialog->setObjectName("shortcutsDialog");dialog->setWindowTitle("Keyboard shortcuts · Space Lens");dialog->setAttribute(Qt::WA_DeleteOnClose);dialog->setWindowModality(Qt::WindowModal);
    auto px=[scale](int value){return qRound(value*scale);};
    auto body=new QVBoxLayout(dialog);body->setContentsMargins(px(24),px(22),px(24),px(20));body->setSpacing(px(16));
    auto title=new QLabel("Keyboard shortcuts",dialog);title->setObjectName("title");body->addWidget(title);
    auto subtitle=new QLabel("Move through your workspace without leaving the keyboard.",dialog);subtitle->setObjectName("muted");subtitle->setWordWrap(true);body->addWidget(subtitle);
    auto groups=new QHBoxLayout();groups->setSpacing(px(16));
    auto group=[&](const char* heading,std::initializer_list<std::pair<const char*,const char*>> rows){
        auto card=new QFrame(dialog);card->setObjectName("shortcutCard");auto layout=new QVBoxLayout(card);layout->setContentsMargins(px(14),px(14),px(14),px(14));layout->setSpacing(px(12));
        auto label=new QLabel(text(heading),card);label->setObjectName("section");layout->addWidget(label);
        auto grid=new QGridLayout();grid->setHorizontalSpacing(px(12));grid->setVerticalSpacing(px(10));int row=0;
        for(auto pair:rows){auto action=new QLabel(text(pair.first),card);auto key=new QLabel(text(pair.second),card);key->setObjectName("shortcutKey");key->setAlignment(Qt::AlignCenter);grid->addWidget(action,row,0);grid->addWidget(key,row++,1,Qt::AlignRight);}
        grid->setColumnStretch(0,1);layout->addLayout(grid);groups->addWidget(card,1);
    };
    group("Navigation",{{"Choose folder","Ctrl + O"},{"Back","Alt + Left"},{"Forward","Alt + Right"},{"Parent folder","Alt + Up"},{"Refresh scan","F5"},{"Cancel / exit map","Esc"}});
    group("Views & selection",{{"Show folders","Ctrl + 1"},{"Show files","Ctrl + 2"},{"Show all items","Ctrl + 3"},{"Open folder","Enter"},{"Copy path","Ctrl + C"},{"Delete with Windows","Del"}});
    body->addLayout(groups);
    auto hint=new QLabel("Space map: scroll to zoom at the pointer · Drag to pan\n+ / − to zoom · 0 to fit",dialog);hint->setObjectName("muted");hint->setWordWrap(true);body->addWidget(hint);
    auto bottom=new QHBoxLayout();bottom->addStretch();auto done=new QPushButton("Done",dialog);done->setDefault(true);QObject::connect(done,&QPushButton::clicked,dialog,&QDialog::accept);bottom->addWidget(done);body->addLayout(bottom);
    dialog->setMinimumWidth(qMin(px(720),owner->screen()->availableGeometry().width()-px(32)));dialog->adjustSize();dialog->move(owner->window()->frameGeometry().center()-dialog->rect().center());dialog->open();
}

void sl_message(void* widget,const char* title,const char* message){QMessageBox box(QMessageBox::Information,text(title),{},QMessageBox::Ok,static_cast<QWidget*>(widget));box.setTextFormat(Qt::PlainText);box.setText(text(message));box.exec();}

void sl_tooltip(void* widget,double x,double y,const char* value){if(!value||!*value){QToolTip::hideText();return;}auto w=static_cast<QWidget*>(widget);QToolTip::showText(w->mapToGlobal(QPoint(qRound(x),qRound(y))),text(value).toHtmlEscaped().replace("\n","<br>"),w);}

void sl_update(void* widget){static_cast<QWidget*>(widget)->update();}

void sl_range(void* widget,int minimum,int maximum,int value){auto o=static_cast<QObject*>(widget);QSignalBlocker block(o);if(auto slider=qobject_cast<QSlider*>(o)){slider->setRange(minimum,maximum);slider->setValue(value);}else if(auto spin=qobject_cast<QSpinBox*>(o)){spin->setRange(minimum,maximum);spin->setValue(value);spin->setSuffix("%");}else if(auto bar=qobject_cast<QProgressBar*>(o)){bar->setRange(minimum,maximum);bar->setValue(value);bar->setTextVisible(false);}}

void sl_progress(int milliseconds){auto start=QElapsedTimer();start.start();while(start.elapsed()<milliseconds){application->processEvents(QEventLoop::AllEvents,5);QThread::msleep(1);}}



void sl_fade(void* widget,double from,double to,int duration){auto w=static_cast<QWidget*>(widget);for(auto prior:w->findChildren<QPropertyAnimation*>(QString(),Qt::FindDirectChildrenOnly)){if(prior->propertyName()=="windowOpacity"){prior->stop();prior->deleteLater();}}auto animation=new QPropertyAnimation(w,"windowOpacity",w);animation->setStartValue(from);animation->setEndValue(to);animation->setDuration(duration);animation->setEasingCurve(QEasingCurve::OutCubic);animation->start(QAbstractAnimation::DeleteWhenStopped);}

void sl_present(void* widget,int animated){auto w=static_cast<QWidget*>(widget);const double from=w->isVisible()?w->windowOpacity():0.;for(auto prior:w->findChildren<QPropertyAnimation*>(QString(),Qt::FindDirectChildrenOnly)){if(prior->propertyName()=="windowOpacity"){prior->stop();prior->deleteLater();}}w->setEnabled(true);w->setWindowOpacity(animated?from:1.);w->showFullScreen();w->raise();if(animated)sl_fade(w,from,1.,200);}

void sl_exit(void* widget,int duration){auto w=static_cast<QWidget*>(widget);for(auto animation:w->findChildren<QPropertyAnimation*>(QString(),Qt::FindDirectChildrenOnly)){if(animation->propertyName()=="windowOpacity"){animation->stop();animation->deleteLater();}}if(!duration){w->hide();return;}w->setEnabled(false);auto animation=new QPropertyAnimation(w,"windowOpacity",w);animation->setStartValue(w->windowOpacity());animation->setEndValue(0.);animation->setDuration(duration);animation->setEasingCurve(QEasingCurve::OutCubic);QObject::connect(animation,&QPropertyAnimation::finished,w,&QWidget::hide);animation->start(QAbstractAnimation::DeleteWhenStopped);}

void sl_outline(void* p,double x,double y,double w,double h,const char* color,double radius,double width){auto painter=static_cast<QPainter*>(p);painter->setRenderHint(QPainter::Antialiasing);painter->setPen(QPen(QColor(text(color)),width));painter->setBrush(Qt::NoBrush);painter->drawRoundedRect(QRectF(x,y,w,h),radius,radius);}

void* sl_snapshot(void* widget,int duration){auto w=static_cast<QWidget*>(widget);auto overlay=new QLabel(w);overlay->setPixmap(w->grab());overlay->setGeometry(w->rect());overlay->setAttribute(Qt::WA_TransparentForMouseEvents);overlay->show();overlay->raise();auto effect=new QGraphicsOpacityEffect(overlay);overlay->setGraphicsEffect(effect);auto animation=new QPropertyAnimation(effect,"opacity",overlay);animation->setStartValue(1.);animation->setEndValue(0.);animation->setDuration(duration);animation->setEasingCurve(QEasingCurve::OutCubic);QObject::connect(animation,&QPropertyAnimation::finished,overlay,&QObject::deleteLater);animation->start();return overlay;}

void sl_fill(void* p,double x,double y,double w,double h,const char* color,double radius){auto painter=static_cast<QPainter*>(p);if(radius>0){painter->setRenderHint(QPainter::Antialiasing);painter->setPen(Qt::NoPen);painter->setBrush(QColor(text(color)));painter->drawRoundedRect(QRectF(x,y,w,h),radius,radius);}else painter->fillRect(QRectF(x,y,w,h),QColor(text(color)));}

void sl_save(void* p){static_cast<QPainter*>(p)->save();}

void sl_restore(void* p){static_cast<QPainter*>(p)->restore();}

void sl_clip(void* p,double x,double y,double w,double h){static_cast<QPainter*>(p)->setClipRect(QRectF(x,y,w,h),Qt::IntersectClip);}

void sl_translate(void* p,double x,double y,double angle){auto painter=static_cast<QPainter*>(p);painter->translate(x,y);painter->rotate(angle);}

double sl_text_width(void* p,const char* value){return static_cast<QPainter*>(p)->fontMetrics().horizontalAdvance(text(value));}

double sl_line_height(void* p){return static_cast<QPainter*>(p)->fontMetrics().height();}

void sl_text(void* p,double x,double y,double w,double h,const char* value,const char* color,int right,int elide){

    auto painter=static_cast<QPainter*>(p);painter->setPen(QColor(text(color)));auto string=text(value);if(elide)string=painter->fontMetrics().elidedText(string,elide==1?Qt::ElideMiddle:Qt::ElideRight,qMax(0,int(w)));painter->drawText(QRectF(x,y,w,h),(right?Qt::AlignRight:Qt::AlignLeft)|Qt::AlignVCenter,string);

}

void sl_map_fill(void* painter,double x,double y,double w,double h,int color,int dark){static const QColor colors[2][5]={{QColor("#d9e9ed"),QColor("#c4dce2"),QColor("#accdd6"),QColor("#e8f1f3"),QColor("#e8eaec")},{QColor("#29434d"),QColor("#335661"),QColor("#3e6570"),QColor("#263c49"),QColor("#353538")}};static_cast<QPainter*>(painter)->fillRect(QRectF(x,y,w,h),colors[dark?1:0][qBound(0,color,4)]);}

uint64_t sl_font_key(void* painter){return qHash(static_cast<QPainter*>(painter)->font());}

void* sl_text_run_new(const unsigned char* name,size_t nameSize,const unsigned char* value,size_t valueSize){auto run=new TextRun{};run->name=QString::fromUtf8(reinterpret_cast<const char*>(name),qsizetype(nameSize));run->value=QString::fromUtf8(reinterpret_cast<const char*>(value),qsizetype(valueSize));return run;}

void sl_text_run_free(void* run){delete static_cast<TextRun*>(run);}

double sl_text_run_width(void* painter,void* value){auto p=static_cast<QPainter*>(painter);auto run=static_cast<TextRun*>(value);if(run->font!=p->font()||run->width<=0){run->font=p->font();run->width=p->fontMetrics().horizontalAdvance(run->name);run->nameLimit=run->valueLimit=-1;}return run->width;}

void sl_text_run_draw(void* painter,void* value,double x,double y,double w,double h,int secondary,int dark){auto p=static_cast<QPainter*>(painter);auto run=static_cast<TextRun*>(value);const int limit=qMax(0,int(w));auto& previous=secondary?run->valueLimit:run->nameLimit;auto& result=secondary?run->elidedValue:run->elidedName;if(previous!=limit){result=p->fontMetrics().elidedText(secondary?run->value:run->name,secondary?Qt::ElideRight:Qt::ElideMiddle,limit);previous=limit;}static const QColor colors[2][2]={{QColor("#153c46"),QColor("#355a63")},{QColor("#ecf4f7"),QColor("#c8dfe5")}};p->setPen(colors[dark?1:0][secondary?1:0]);p->drawText(QRectF(x,y,w,h),Qt::AlignLeft|Qt::AlignVCenter,result);}






void sl_font(void* p,double pixels,int bold){auto painter=static_cast<QPainter*>(p);auto font=painter->font();font.setPixelSize(qMax(1,qRound(pixels)));font.setBold(bold);painter->setFont(font);}

void sl_line(void* p,double x1,double y1,double x2,double y2,const char* color,double width){auto painter=static_cast<QPainter*>(p);painter->setRenderHint(QPainter::Antialiasing);painter->setPen(QPen(QColor(text(color)),width,Qt::SolidLine,Qt::RoundCap));painter->drawLine(QPointF(x1,y1),QPointF(x2,y2));}

}

